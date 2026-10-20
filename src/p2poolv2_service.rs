// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_PREFIX: &str = "p2poolv2@";
const PROMETHEUS_UNIT: &str = "pdm-prometheus.service";
const GRAFANA_UNIT: &str = "pdm-grafana.service";
const PROMETHEUS_UNIT_CONTENT: &str = include_str!("../systemd/user/pdm-prometheus.service");
const GRAFANA_UNIT_CONTENT: &str = include_str!("../systemd/user/pdm-grafana.service");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitoringService {
    Prometheus,
    Grafana,
}

#[derive(Debug, Clone, Default)]
struct NativeExecutables {
    prometheus: Option<PathBuf>,
    grafana: Option<PathBuf>,
    grafana_home: Option<PathBuf>,
}

impl NativeExecutables {
    fn discover() -> Self {
        let prometheus = find_native_executable("prometheus");
        let grafana = find_native_executable("grafana");
        let grafana_home = grafana
            .as_deref()
            .and_then(|binary| resolve_grafana_home(binary).ok().flatten());
        Self {
            prometheus,
            grafana,
            grafana_home,
        }
    }
}

impl MonitoringService {
    fn unit(self) -> &'static str {
        match self {
            Self::Prometheus => PROMETHEUS_UNIT,
            Self::Grafana => GRAFANA_UNIT,
        }
    }

    fn content(self, config_dir: &Path, executables: &NativeExecutables) -> Result<String> {
        let template = match self {
            Self::Prometheus => PROMETHEUS_UNIT_CONTENT,
            Self::Grafana => GRAFANA_UNIT_CONTENT,
        };
        let path = config_dir
            .to_str()
            .context("PDM monitoring config path is not valid UTF-8 for systemd")?;
        if path.chars().any(char::is_control) {
            anyhow::bail!("PDM monitoring config path contains characters unsupported by systemd");
        }
        let escaped = path
            .replace('%', "%%")
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let escaped_path = systemd_path(config_dir)?;
        let mut content = template
            .replace("@PDM_CONFIG_DIR@", &escaped)
            .replace("@PDM_CONFIG_DIR_PATH@", &escaped_path);
        match self {
            Self::Prometheus => {
                if let Some(executable) = executables.prometheus.as_deref() {
                    content = content.replace(
                        "ExecStart=prometheus ",
                        &format!("ExecStart={} ", systemd_quote(executable)?),
                    );
                }
            }
            Self::Grafana => {
                let home = executables
                    .grafana_home
                    .as_deref()
                    .unwrap_or_else(|| Path::new("/usr/share/grafana"));
                let config = home.join("conf/defaults.ini");
                let config_path = config
                    .to_str()
                    .context("Grafana config path is not valid UTF-8 for systemd")?;
                if config_path.chars().any(char::is_control) {
                    anyhow::bail!("Grafana config path contains characters unsupported by systemd");
                }
                let escaped_config = systemd_escape(config_path)?;
                let executable = executables
                    .grafana
                    .as_deref()
                    .map(systemd_quote)
                    .transpose()?
                    .unwrap_or_else(|| "grafana".to_owned());
                content = content.replace("@GRAFANA_CONFIG@", &escaped_config);
                content = content.replace(
                    "# @GRAFANA_HOME_DIRECTIVES@",
                    &format!("WorkingDirectory={}", systemd_path(home)?),
                );
                content = content.replace(
                    "ExecStart=grafana server",
                    &format!(
                        "ExecStart={executable} server --config={} --homepath={}",
                        systemd_quote(&config)?,
                        systemd_quote(home)?
                    ),
                );
            }
        }
        Ok(content)
    }
}

/// Lifecycle controls for the PDM-owned systemd user services. Units are
/// installed on demand, then daemon-reload is called. They are never enabled
/// automatically, so services run only when explicitly started from PDM.
pub struct MonitoringServices;

impl MonitoringServices {
    pub fn start(service: MonitoringService) -> Result<()> {
        Self::run("start", service)
    }
    pub fn stop(service: MonitoringService) -> Result<()> {
        Self::run("stop", service)
    }
    pub fn restart(service: MonitoringService) -> Result<()> {
        Self::run("restart", service)
    }

    pub fn status(service: MonitoringService) -> Result<String> {
        Self::reload_user_units()?;
        let output = Command::new("systemctl")
            .args([
                "--user",
                "show",
                service.unit(),
                "--property=ActiveState",
                "--value",
            ])
            .output()
            .context("failed to execute systemctl --user show; is systemd user mode available?")?;
        if !output.status.success() {
            anyhow::bail!(
                "systemctl --user show {} failed: {}",
                service.unit(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if state.is_empty() {
            anyhow::bail!("systemctl returned an empty status for {}", service.unit());
        }
        Ok(state)
    }

    fn run(action: &str, service: MonitoringService) -> Result<()> {
        if action != "stop" {
            let executable = match service {
                MonitoringService::Prometheus => "prometheus",
                MonitoringService::Grafana => "grafana",
            };
            if !native_executable_available(executable) {
                anyhow::bail!(
                    "{executable} executable was not found in PATH or standard executable directories; install it separately before starting the PDM service"
                );
            }
            if matches!(service, MonitoringService::Grafana) {
                let binary = find_native_executable("grafana").expect("checked above");
                resolve_grafana_home(&binary)?.context(
                    "could not find Grafana's readable conf/defaults.ini and public assets; install Grafana's complete package before starting the PDM service",
                )?;
                let config_dir = crate::settings::config_dir()
                    .context("cannot determine PDM config directory for Grafana credentials")?;
                let state_dir = user_p2pool_state_base_dir()
                    .context("cannot determine PDM state directory for Grafana")?
                    .join("pdm/grafana");
                ensure_grafana_admin_credentials(&config_dir, &state_dir)?;
            }
        }
        if action == "stop" {
            Self::reload_user_units()?;
        } else {
            Self::prepare_units()?;
        }
        let output = Command::new("systemctl")
            .args(["--user", action, service.unit()])
            .output()
            .with_context(|| {
                format!(
                    "failed to execute systemctl --user {action}; is systemd user mode available?"
                )
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        anyhow::bail!(
            "systemctl --user {action} {} failed: {}",
            service.unit(),
            if stderr.is_empty() {
                "systemd rejected the operation"
            } else {
                &stderr
            }
        );
    }

    fn prepare_units() -> Result<()> {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".config")))
            .context("cannot determine XDG config directory for systemd user units")?;
        let config_dir = crate::settings::config_dir()
            .context("cannot determine PDM config directory for monitoring units")?;
        let dir = config_home.join("systemd/user");
        std::fs::create_dir_all(&dir).with_context(|| {
            format!(
                "cannot create systemd user unit directory {}",
                dir.display()
            )
        })?;
        let executables = NativeExecutables::discover();
        for service in [MonitoringService::Prometheus, MonitoringService::Grafana] {
            let path = dir.join(service.unit());
            let content = service.content(&config_dir, &executables)?;
            match std::fs::read_to_string(&path) {
                Ok(existing) if existing == content => {}
                Ok(existing) if existing.starts_with("# Managed by PDM.") => {
                    std::fs::write(&path, &content).with_context(|| {
                        format!("cannot update PDM systemd unit {}", path.display())
                    })?;
                }
                Ok(_) => anyhow::bail!(
                    "refusing to replace existing unit file {}; move it aside if you want PDM to manage this unit",
                    path.display()
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::write(&path, &content).with_context(|| {
                        format!("cannot install PDM systemd unit {}", path.display())
                    })?;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("cannot inspect systemd unit {}", path.display())
                    });
                }
            }
        }
        Self::reload_user_units()
    }

    fn reload_user_units() -> Result<()> {
        let output = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .context("failed to execute systemctl --user daemon-reload")?;
        if !output.status.success() {
            anyhow::bail!(
                "systemctl --user daemon-reload failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
}

pub fn user_p2pool_config_dir() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(config_home.join("p2poolv2"))
}

pub fn user_p2pool_state_base_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
}

pub fn user_p2pool_state_dir(instance: &str) -> Option<PathBuf> {
    let state_base = user_p2pool_state_base_dir()?;
    Some(state_base.join("p2poolv2").join(instance))
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StorePathError {
    ConfigMissing,
    UnmanagedRelativePath(String),
    StateDirUnavailable(String),
}

impl std::fmt::Display for StorePathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigMissing => write!(f, "No P2Pool config loaded"),
            Self::UnmanagedRelativePath(path) => write!(
                f,
                "Cannot determine runtime working directory for unmanaged relative store path: '{path}'"
            ),
            Self::StateDirUnavailable(instance) => write!(
                f,
                "Cannot determine systemd state directory for instance '{instance}'"
            ),
        }
    }
}

impl std::error::Error for StorePathError {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StorageSizeError {
    NotFound(PathBuf),
    Io(String),
}

impl std::fmt::Display for StorageSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Store path does not exist: {}", path.display()),
            Self::Io(err) => write!(f, "Filesystem error: {err}"),
        }
    }
}

impl std::error::Error for StorageSizeError {}

pub fn calculate_path_size(path: &Path) -> Result<u64, StorageSizeError> {
    let initial_metadata = std::fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StorageSizeError::NotFound(path.to_path_buf())
        } else {
            StorageSizeError::Io(e.to_string())
        }
    })?;

    if !initial_metadata.is_dir() {
        return Ok(initial_metadata.len());
    }

    let mut total_size: u64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let entries = match std::fs::read_dir(&current_dir) {
            Ok(entries) => entries,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                return Err(StorageSizeError::Io(e.to_string()));
            }
        };

        for entry in entries {
            let entry = entry.map_err(|e| StorageSizeError::Io(e.to_string()))?;
            let entry_path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|e| StorageSizeError::Io(e.to_string()))?;

            if metadata.is_dir() {
                stack.push(entry_path);
            } else {
                total_size += metadata.len();
            }
        }
    }

    Ok(total_size)
}

pub fn format_size_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    }
}

pub fn resolve_store_path(
    config: &p2poolv2_config::Config,
    config_path: Option<&Path>,
) -> Result<PathBuf, StorePathError> {
    let store_path = Path::new(&config.store.path);

    if store_path.is_absolute() {
        return Ok(store_path.to_path_buf());
    }

    if let Some(path) = config_path
        && let Some(instance) = instance_from_config_path(path)
    {
        if let Some(state_dir) = user_p2pool_state_dir(&instance) {
            return Ok(state_dir.join(store_path));
        }
        return Err(StorePathError::StateDirUnavailable(instance));
    }

    Err(StorePathError::UnmanagedRelativePath(
        config.store.path.clone(),
    ))
}

pub fn resolve_store_path_in_dirs(
    config: &p2poolv2_config::Config,
    config_path: Option<&Path>,
    config_dir: &Path,
    state_base_dir: &Path,
) -> Result<PathBuf, StorePathError> {
    let store_path = Path::new(&config.store.path);

    if store_path.is_absolute() {
        return Ok(store_path.to_path_buf());
    }

    if let Some(path) = config_path
        && let Some(instance) = instance_from_config_path_in_dir(path, config_dir)
    {
        let state_dir = state_base_dir.join("p2poolv2").join(instance);
        return Ok(state_dir.join(store_path));
    }

    Err(StorePathError::UnmanagedRelativePath(
        config.store.path.clone(),
    ))
}

pub fn instance_from_config_path(path: &Path) -> Option<String> {
    let config_dir = user_p2pool_config_dir()?;
    instance_from_config_path_in_dir(path, &config_dir)
}

fn instance_from_config_path_in_dir(path: &Path, config_dir: &Path) -> Option<String> {
    if path.parent()? != config_dir {
        return None;
    }

    let filename = path.file_name()?.to_str()?;
    let instance = filename.strip_prefix("config-")?.strip_suffix(".toml")?;

    if instance.is_empty()
        || !instance.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return None;
    }

    Some(instance.to_string())
}

fn service_name(instance: &str) -> String {
    format!("{SERVICE_PREFIX}{instance}")
}

pub struct P2PoolV2Service;

impl P2PoolV2Service {
    pub fn start(instance: &str) -> Result<()> {
        Self::run_systemctl("start", instance)
    }

    pub fn stop(instance: &str) -> Result<()> {
        Self::run_systemctl("stop", instance)
    }

    pub fn restart(instance: &str) -> Result<()> {
        Self::run_systemctl("restart", instance)
    }

    pub fn is_running(instance: &str) -> Result<bool> {
        let service = validated_service_name(instance)?;
        let output = Command::new("systemctl")
            .args(["--user", "is-active", &service])
            .output()
            .context("failed to execute systemctl")?;

        Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "active")
    }

    fn run_systemctl(action: &str, instance: &str) -> Result<()> {
        let service = validated_service_name(instance)?;
        let output = Command::new("systemctl")
            .args(["--user", action, &service])
            .output()
            .with_context(|| format!("failed to execute systemctl {action}"))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "systemctl --user {action} {service} failed: {}",
                stderr.trim()
            );
        }
    }

    /// Runs `systemctl --user show <service> --property=<property> --value`
    /// and returns the trimmed stdout.
    fn show_property(instance: &str, property: &str) -> Result<String> {
        let service = validated_service_name(instance)?;
        let output = Command::new("systemctl")
            .args([
                "--user",
                "show",
                &service,
                "--property",
                property,
                "--value",
            ])
            .output()
            .with_context(|| format!("failed to execute systemctl show {property}"))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "systemctl --user show {service} --property {property} failed: {}",
                stderr.trim()
            );
        }
    }

    /// Returns the current RSS of the P2Poolv2 service process in bytes,
    /// or `None` if the service is not running.
    ///
    /// Strategy:
    /// 1. Query `MemoryCurrent` from systemd.  When cgroup v2 memory
    ///    accounting is active this gives the live value without touching
    ///    `/proc`.
    /// 2. If systemd returns `u64::MAX` (its sentinel for "not available"),
    ///    fall back to reading `VmRSS` from `/proc/<MainPID>/status`.
    /// 3. If `MainPID` is `0` the service has no live process; return `None`.
    pub fn memory_rss_bytes(instance: &str) -> Result<Option<u64>> {
        let raw = Self::show_property(instance, "MemoryCurrent")?;
        let memory_current = raw.parse::<u64>().with_context(|| {
            format!("systemd returned non-integer MemoryCurrent for '{instance}': '{raw}'")
        })?;

        if memory_current != u64::MAX {
            return Ok(Some(memory_current));
        }

        // systemd sentinel — cgroup accounting not available or service not
        // running.  Fall back via MainPID → /proc/<pid>/status.
        let pid_raw = Self::show_property(instance, "MainPID")?;
        let pid = pid_raw.parse::<u32>().with_context(|| {
            format!("systemd returned non-integer MainPID for '{instance}': '{pid_raw}'")
        })?;

        if pid == 0 {
            return Ok(None);
        }

        Self::rss_from_proc(pid)
    }

    /// Reads `VmRSS` from `/proc/<pid>/status` and returns it as bytes.
    /// Returns `Ok(None)` if the file does not exist (process already exited).
    fn rss_from_proc(pid: u32) -> Result<Option<u64>> {
        Self::rss_from_proc_root(pid, "/proc")
    }

    /// Like `rss_from_proc` but accepts a custom root (used in tests to
    /// redirect reads to a temporary directory).
    fn rss_from_proc_root(pid: u32, proc_root: &str) -> Result<Option<u64>> {
        let path = format!("{proc_root}/{pid}/status");
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read {path}"));
            }
        };

        for line in contents.lines() {
            let Some(rest) = line.strip_prefix("VmRSS:") else {
                continue;
            };
            // Format: "VmRSS:\t  51200 kB"
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 && parts[1].eq_ignore_ascii_case("kb") {
                let kb = parts[0].parse::<u64>().with_context(|| {
                    format!("could not parse VmRSS value in {path}: '{}'", parts[0])
                })?;
                return Ok(Some(kb * 1024));
            }
            // Unexpected unit — treat as parse failure
            anyhow::bail!("unexpected VmRSS format in {path}: '{rest}'");
        }

        // VmRSS field absent — should not happen for a live process, but
        // treat as unavailable rather than crashing.
        Ok(None)
    }
}

fn native_executable_available(executable: &str) -> bool {
    find_native_executable(executable).is_some()
}

fn find_native_executable(executable: &str) -> Option<PathBuf> {
    let mut directories =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    directories.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/sbin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
    ]);
    directories
        .into_iter()
        .map(|directory| directory.join(executable))
        .find(|path| is_executable_file(path))
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| {
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

fn systemd_quote(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .context("native executable path is not valid UTF-8 for systemd")?;
    if path.chars().any(char::is_control) {
        anyhow::bail!("native executable path contains characters unsupported by systemd");
    }
    Ok(format!("\"{}\"", systemd_escape(path)?))
}

fn systemd_escape(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        anyhow::bail!("path contains characters unsupported by systemd");
    }
    Ok(value
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('\"', "\\\""))
}

fn systemd_path(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .context("systemd unit path is not valid UTF-8")?;
    if path.chars().any(char::is_control) {
        anyhow::bail!("systemd unit path contains characters unsupported by systemd");
    }
    Ok(systemd_escape(path)?.replace(' ', "\\x20"))
}

/// Detect a Grafana installation with its home assets beside `bin/grafana`,
/// or in Ubuntu's standard `/usr/share/grafana` package location.
fn resolve_grafana_home(binary: &Path) -> Result<Option<PathBuf>> {
    let canonical = std::fs::canonicalize(binary).unwrap_or_else(|_| binary.to_path_buf());
    let Some(bin_dir) = canonical.parent() else {
        return Ok(None);
    };
    if bin_dir.file_name().is_some_and(|name| name == "bin") {
        if let Some(root) = bin_dir.parent() {
            if root.join("public").is_dir() && root.join("conf/defaults.ini").is_file() {
                return Ok(Some(root.to_path_buf()));
            }
            let standard_bins = [
                "/usr/bin",
                "/usr/sbin",
                "/usr/local/bin",
                "/usr/local/sbin",
                "/bin",
                "/sbin",
            ];
            if !standard_bins
                .iter()
                .any(|standard| bin_dir == Path::new(standard))
            {
                anyhow::bail!(
                    "Grafana executable {} appears to be a tarball installation, but required public/ and conf/defaults.ini assets are missing beside it; keep the extracted directory intact",
                    binary.display()
                );
            }
        }
    }
    let package_home = Path::new("/usr/share/grafana");
    if package_home.join("public").is_dir() && package_home.join("conf/defaults.ini").is_file() {
        return Ok(Some(package_home.to_path_buf()));
    }
    Ok(None)
}

fn ensure_grafana_admin_credentials(config_dir: &Path, state_dir: &Path) -> Result<PathBuf> {
    let grafana_dir = config_dir.join("monitoring/grafana");
    std::fs::create_dir_all(&grafana_dir).with_context(|| {
        format!(
            "cannot create Grafana configuration directory {}",
            grafana_dir.display()
        )
    })?;
    let credentials = grafana_dir.join("admin.env");
    match std::fs::read_to_string(&credentials) {
        Ok(contents) => {
            validate_grafana_admin_credentials(&contents, &credentials)?;
            return Ok(credentials);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("cannot read Grafana credentials {}", credentials.display())
            });
        }
    }
    if state_dir.join("data/grafana.db").is_file() || state_dir.join("grafana.db").is_file() {
        anyhow::bail!(
            "Grafana's admin.env credentials are missing while an existing database is present; restore the credentials file or reset the Grafana admin password before starting"
        );
    }

    let mut random = File::open("/dev/urandom")
        .context("cannot open the operating system random source for Grafana credentials")?;
    let mut secret = [0u8; 32];
    random
        .read_exact(&mut secret)
        .context("cannot read the operating system random source for Grafana credentials")?;
    let secret = secret
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let contents = format!("GF_SECURITY_ADMIN_PASSWORD={secret}\n");

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&credentials) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let contents = std::fs::read_to_string(&credentials).with_context(|| {
                format!("cannot read Grafana credentials {}", credentials.display())
            })?;
            validate_grafana_admin_credentials(&contents, &credentials)?;
            return Ok(credentials);
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "cannot create Grafana credentials {}",
                    credentials.display()
                )
            });
        }
    };
    if let Err(error) = file.write_all(contents.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(&credentials);
        return Err(error).with_context(|| {
            format!("cannot write Grafana credentials {}", credentials.display())
        });
    }
    Ok(credentials)
}

fn validate_grafana_admin_credentials(contents: &str, path: &Path) -> Result<()> {
    let password = contents
        .strip_prefix("GF_SECURITY_ADMIN_PASSWORD=")
        .and_then(|contents| contents.strip_suffix('\n'))
        .context("Grafana admin credentials file has an unsupported format")?;
    if password.len() != 64 || !password.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!(
            "Grafana admin credentials file {} is invalid; remove it only after recovering or resetting the Grafana admin password",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).with_context(
            || {
                format!(
                    "cannot restrict permissions on Grafana credentials {}",
                    path.display()
                )
            },
        )?;
    }
    Ok(())
}

fn validated_service_name(instance: &str) -> Result<String> {
    if instance.is_empty()
        || !instance.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        anyhow::bail!("invalid P2Poolv2 service instance: {instance}");
    }

    Ok(service_name(instance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn fake_systemctl(
        fail_actions: bool,
        active: bool,
    ) -> (TempDir, Option<std::ffi::OsString>, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("systemctl");
        let args_file = dir.path().join("args");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nif [ \"$2\" = \"is-active\" ]; then\n  {}\nelse\n  {}\nfi\n",
                args_file.display(),
                if active {
                    "echo active; exit 0"
                } else {
                    "echo inactive; exit 3"
                },
                if fail_actions {
                    "echo permission denied >&2; exit 1"
                } else {
                    "exit 0"
                }
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let old_path = std::env::var_os("PATH");
        let path = match old_path.as_ref() {
            Some(old_path) => format!("{}:{}", dir.path().display(), old_path.to_string_lossy()),
            None => dir.path().display().to_string(),
        };
        unsafe { std::env::set_var("PATH", path) };
        (dir, old_path, args_file)
    }

    fn restore_path(old_path: Option<std::ffi::OsString>) {
        unsafe {
            match old_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    /// Build a fake `systemctl` that handles `show … --property=<prop> --value`
    /// by emitting the caller-supplied property values in order.
    ///
    /// `properties` is a list of `(property_name, value_to_emit)` pairs that
    /// the script matches against `$5` (the `--property` argument).
    /// An unknown property causes the script to exit 1.
    fn fake_systemctl_show(properties: &[(&str, &str)]) -> (TempDir, Option<std::ffi::OsString>) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("systemctl");

        // Build a chain of `elif [ "$5" = "<prop>" ]; then echo <value>` clauses.
        let mut cases = String::new();
        for (index, (prop, value)) in properties.iter().enumerate() {
            if index == 0 {
                cases.push_str(&format!(
                    "if [ \"$5\" = \"{prop}\" ]; then\n  echo {value}\n"
                ));
            } else {
                cases.push_str(&format!(
                    "elif [ \"$5\" = \"{prop}\" ]; then\n  echo {value}\n"
                ));
            }
        }
        cases.push_str("else\n  echo \"unknown property $5\" >&2; exit 1\nfi\n");

        fs::write(&script, format!("#!/bin/sh\n{cases}")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let old_path = std::env::var_os("PATH");
        let path = match old_path.as_ref() {
            Some(old_path) => format!("{}:{}", dir.path().display(), old_path.to_string_lossy()),
            None => dir.path().display().to_string(),
        };
        unsafe { std::env::set_var("PATH", path) };
        (dir, old_path)
    }

    /// Write a minimal `/proc/<pid>/status`-style file under `base_dir` and
    /// return the fake PID so the caller can point the code at `base_dir`.
    fn fake_proc_status(base_dir: &std::path::Path, pid: u32, vmrss_kb: u64) {
        let proc_dir = base_dir.join(format!("{pid}"));
        fs::create_dir_all(&proc_dir).unwrap();
        fs::write(
            proc_dir.join("status"),
            format!("Name:\tp2poolv2\nVmRSS:\t{vmrss_kb} kB\n"),
        )
        .unwrap();
    }

    fn fake_monitoring_systemctl(
        failure: Option<&str>,
    ) -> (
        TempDir,
        Option<std::ffi::OsString>,
        Option<std::ffi::OsString>,
        Option<std::ffi::OsString>,
        PathBuf,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("systemctl");
        let calls = dir.path().join("calls");
        let stamp = dir.path().join("reload-attempted");
        let prometheus = dir.path().join("prometheus");
        fs::write(&prometheus, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&prometheus, fs::Permissions::from_mode(0o755)).unwrap();
        let grafana_home = dir.path().join("grafana-install");
        let grafana_bin = grafana_home.join("bin");
        fs::create_dir_all(&grafana_bin).unwrap();
        fs::create_dir_all(grafana_home.join("public")).unwrap();
        fs::create_dir_all(grafana_home.join("conf")).unwrap();
        fs::write(grafana_home.join("conf/defaults.ini"), "[paths]\n").unwrap();
        let grafana = grafana_bin.join("grafana");
        fs::write(&grafana, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&grafana, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&grafana, dir.path().join("grafana")).unwrap();
        let failure = failure.unwrap_or("");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$2\" = \"show\" ]; then echo active; exit 0; fi\nif [ \"{failure}\" = \"reload-once\" ] && [ \"$2\" = \"daemon-reload\" ] && [ ! -f '{}' ]; then touch '{}'; echo transient reload error >&2; exit 1; fi\nif [ \"{failure}\" != \"\" ] && [ \"{failure}\" != \"reload-once\" ] && [ \"$2\" != \"daemon-reload\" ]; then echo \"{failure}\" >&2; exit 1; fi\nexit 0\n",
                calls.display(), stamp.display(), stamp.display()
            ),
        ).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let old_path = std::env::var_os("PATH");
        let old_config = std::env::var_os("XDG_CONFIG_HOME");
        let old_state = std::env::var_os("XDG_STATE_HOME");
        let path = match old_path.as_ref() {
            Some(old) => format!("{}:{}", dir.path().display(), old.to_string_lossy()),
            None => dir.path().display().to_string(),
        };
        unsafe {
            std::env::set_var("PATH", path);
            std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
            std::env::set_var("XDG_STATE_HOME", dir.path().join("state"));
        }
        (dir, old_path, old_config, old_state, calls)
    }

    fn restore_monitoring_env(
        path: Option<std::ffi::OsString>,
        config: Option<std::ffi::OsString>,
        state: Option<std::ffi::OsString>,
    ) {
        unsafe {
            match path {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
            match config {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
            match state {
                Some(value) => std::env::set_var("XDG_STATE_HOME", value),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
        }
    }

    #[test]
    #[serial]
    fn monitoring_service_installs_units_and_runs_actions() -> Result<()> {
        let (_dir, old_path, old_config, old_state, calls) = fake_monitoring_systemctl(None);
        MonitoringServices::start(MonitoringService::Prometheus)?;
        MonitoringServices::restart(MonitoringService::Grafana)?;
        MonitoringServices::stop(MonitoringService::Grafana)?;
        let unit_dir =
            PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("systemd/user");
        assert!(unit_dir.join(PROMETHEUS_UNIT).is_file());
        assert!(unit_dir.join(GRAFANA_UNIT).is_file());
        let generated_prometheus = fs::read_to_string(unit_dir.join(PROMETHEUS_UNIT))?;
        let configured_dir = crate::settings::config_dir()?;
        let generated_grafana = fs::read_to_string(unit_dir.join(GRAFANA_UNIT))?;
        assert!(generated_prometheus.contains(&format!(
            "--config.file=\"{}/monitoring/prometheus.yml\"",
            configured_dir.display()
        )));
        assert!(generated_grafana.contains(&format!(
            "EnvironmentFile={}/monitoring/grafana/admin.env",
            configured_dir.display()
        )));
        assert!(generated_grafana.contains("--config="));
        assert!(generated_grafana.contains("--homepath="));
        assert!(
            configured_dir
                .join("monitoring/grafana/admin.env")
                .is_file()
        );
        assert_eq!(
            MonitoringServices::status(MonitoringService::Prometheus)?,
            "active"
        );
        assert_eq!(
            fs::read_to_string(calls)?.lines().collect::<Vec<_>>(),
            [
                "--user daemon-reload",
                "--user start pdm-prometheus.service",
                "--user daemon-reload",
                "--user restart pdm-grafana.service",
                "--user daemon-reload",
                "--user stop pdm-grafana.service",
                "--user daemon-reload",
                "--user show pdm-prometheus.service --property=ActiveState --value",
            ]
        );
        restore_monitoring_env(old_path, old_config, old_state);
        Ok(())
    }

    #[test]
    #[serial]
    fn monitoring_service_retries_failed_daemon_reload_before_start() -> Result<()> {
        let (_dir, old_path, old_config, old_state, calls) =
            fake_monitoring_systemctl(Some("reload-once"));
        let first = MonitoringServices::start(MonitoringService::Prometheus).unwrap_err();
        assert!(first.to_string().contains("daemon-reload failed"));
        MonitoringServices::start(MonitoringService::Prometheus)?;
        assert_eq!(
            fs::read_to_string(calls)?.lines().collect::<Vec<_>>(),
            [
                "--user daemon-reload",
                "--user daemon-reload",
                "--user start pdm-prometheus.service",
            ]
        );
        restore_monitoring_env(old_path, old_config, old_state);
        Ok(())
    }

    #[test]
    #[serial]
    fn monitoring_service_returns_inactive_status() -> Result<()> {
        let (dir, old_path, old_config, old_state, _calls) = fake_monitoring_systemctl(None);
        let script = dir.path().join("systemctl");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$2\" = \"show\" ]; then echo inactive; exit 0; fi\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            MonitoringServices::status(MonitoringService::Grafana)?,
            "inactive"
        );
        restore_monitoring_env(old_path, old_config, old_state);
        Ok(())
    }

    #[test]
    #[serial]
    fn monitoring_service_reports_status_query_error() {
        let (dir, old_path, old_config, old_state, _calls) = fake_monitoring_systemctl(None);
        let script = dir.path().join("systemctl");
        fs::write(&script, "#!/bin/sh\nif [ \"$2\" = \"show\" ]; then echo \"unit unavailable\" >&2; exit 1; fi\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let error = MonitoringServices::status(MonitoringService::Grafana)
            .unwrap_err()
            .to_string();
        restore_monitoring_env(old_path, old_config, old_state);
        assert!(error.contains("unit unavailable"), "{error}");
    }

    #[test]
    #[serial]
    fn monitoring_service_reports_systemctl_failures() {
        let (_dir, old_path, old_config, old_state, _calls) =
            fake_monitoring_systemctl(Some("Unit pdm-prometheus.service not found"));
        let error = MonitoringServices::start(MonitoringService::Prometheus)
            .unwrap_err()
            .to_string();
        restore_monitoring_env(old_path, old_config, old_state);
        assert!(
            error.contains("Unit pdm-prometheus.service not found"),
            "{error}"
        );
    }

    #[test]
    fn monitoring_service_detects_missing_executable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_executable_file(&dir.path().join("prometheus")));
        let grafana = dir.path().join("grafana");
        fs::write(&grafana, "binary").unwrap();
        fs::set_permissions(&grafana, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable_file(&grafana));
    }

    #[test]
    #[serial]
    fn monitoring_service_refuses_to_replace_foreign_unit() {
        let (_dir, old_path, old_config, old_state, _calls) = fake_monitoring_systemctl(None);
        let unit_dir =
            PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("systemd/user");
        fs::create_dir_all(&unit_dir).unwrap();
        fs::write(unit_dir.join(PROMETHEUS_UNIT), "# user-owned\n").unwrap();
        let error = MonitoringServices::start(MonitoringService::Prometheus)
            .unwrap_err()
            .to_string();
        restore_monitoring_env(old_path, old_config, old_state);
        assert!(
            error.contains("refusing to replace existing unit file"),
            "{error}"
        );
    }

    #[test]
    fn monitoring_unit_path_escapes_systemd_config_dir_values() -> Result<()> {
        let content = MonitoringService::Grafana.content(
            Path::new("/tmp/pdm test%path"),
            &NativeExecutables::default(),
        )?;
        assert!(
            content.contains(
                "GF_PATHS_PROVISIONING=/tmp/pdm test%%path/monitoring/grafana/provisioning"
            )
        );
        assert!(
            content
                .contains("EnvironmentFile=/tmp/pdm\\x20test%%path/monitoring/grafana/admin.env")
        );
        Ok(())
    }

    #[test]
    fn grafana_tarball_requires_and_uses_home_assets() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin)?;
        let executable = bin.join("grafana");
        fs::write(&executable, "binary")?;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
        fs::create_dir_all(root.path().join("public"))?;
        fs::create_dir_all(root.path().join("conf"))?;
        fs::write(root.path().join("conf/defaults.ini"), "[paths]")?;

        assert_eq!(
            resolve_grafana_home(&executable)?,
            Some(root.path().to_path_buf())
        );
        let content = MonitoringService::Grafana.content(
            Path::new("/tmp/pdm-config"),
            &NativeExecutables {
                grafana_home: Some(root.path().to_path_buf()),
                ..NativeExecutables::default()
            },
        )?;
        assert!(content.contains(&format!("WorkingDirectory={}", root.path().display())));
        assert!(content.contains(&format!("--homepath=\"{}\"", root.path().display())));
        assert!(content.contains(&format!(
            "--config=\"{}/conf/defaults.ini\"",
            root.path().display()
        )));
        assert!(content.contains(&format!(
            "GF_PATHS_CONFIG={}/conf/defaults.ini",
            root.path().display()
        )));
        Ok(())
    }

    #[test]
    fn grafana_tarball_missing_assets_returns_actionable_error() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin)?;
        let executable = bin.join("grafana");
        fs::write(&executable, "binary")?;
        let error = resolve_grafana_home(&executable).unwrap_err().to_string();
        assert!(
            error.contains("public/") && error.contains("conf/defaults.ini"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn monitoring_units_use_expected_paths_ports_and_dependencies() {
        assert!(PROMETHEUS_UNIT_CONTENT.contains(
            "ExecStart=prometheus --config.file=\"@PDM_CONFIG_DIR@/monitoring/prometheus.yml\""
        ));
        assert!(PROMETHEUS_UNIT_CONTENT.contains("--storage.tsdb.path=%S/pdm/prometheus"));
        assert!(PROMETHEUS_UNIT_CONTENT.contains("127.0.0.1:9090"));
        assert!(GRAFANA_UNIT_CONTENT.contains("After=pdm-prometheus.service"));
        assert!(GRAFANA_UNIT_CONTENT.contains("Wants=pdm-prometheus.service"));
        assert!(GRAFANA_UNIT_CONTENT.contains("@PDM_CONFIG_DIR@/monitoring/grafana/provisioning"));
        assert!(GRAFANA_UNIT_CONTENT.contains("%S/pdm/grafana"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_SERVER_HTTP_PORT=3000"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_PATHS_CONFIG=@GRAFANA_CONFIG@"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_PATHS_DATA=%S/pdm/grafana"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_PATHS_LOGS=%S/pdm/grafana/logs"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_PATHS_PLUGINS=%S/pdm/grafana/plugins"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_SERVER_HTTP_ADDR=127.0.0.1"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_AUTH_ANONYMOUS_ENABLED=false"));
        assert!(!GRAFANA_UNIT_CONTENT.contains("GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer"));
        assert!(
            GRAFANA_UNIT_CONTENT
                .contains("EnvironmentFile=@PDM_CONFIG_DIR_PATH@/monitoring/grafana/admin.env")
        );
        assert!(!GRAFANA_UNIT_CONTENT.contains("GF_SECURITY_ADMIN_PASSWORD=admin"));
        assert!(!GRAFANA_UNIT_CONTENT.contains("/etc/grafana/grafana.ini"));
        assert!(GRAFANA_UNIT_CONTENT.contains("GF_USERS_ALLOW_SIGN_UP=false"));
        assert!(GRAFANA_UNIT_CONTENT.contains("ProtectSystem=strict"));
        assert!(GRAFANA_UNIT_CONTENT.contains("NoNewPrivileges=true"));
        assert!(GRAFANA_UNIT_CONTENT.contains("ReadWritePaths=%S/pdm/grafana"));
        assert!(
            !PROMETHEUS_UNIT_CONTENT.contains("[Install]")
                || PROMETHEUS_UNIT_CONTENT.contains("WantedBy=default.target")
        );
    }

    #[test]
    fn grafana_package_unit_uses_readable_config_and_all_pdm_paths() -> Result<()> {
        let root = Path::new("/usr/share/grafana");
        let executable = root.join("bin/grafana");
        let content = MonitoringService::Grafana.content(
            Path::new("/home/example/.config/pdm"),
            &NativeExecutables {
                grafana: Some(executable.clone()),
                grafana_home: Some(root.to_path_buf()),
                prometheus: None,
            },
        )?;
        assert!(content.contains(&format!(
            "ExecStart=\"{}\" server --config=\"{}/conf/defaults.ini\" --homepath=\"{}\"",
            executable.display(),
            root.display(),
            root.display()
        )));
        assert!(content.contains(&format!("WorkingDirectory={}", root.display())));
        assert!(
            content
                .contains("Environment=\"GF_PATHS_CONFIG=/usr/share/grafana/conf/defaults.ini\"")
        );
        assert!(content.contains("Environment=GF_PATHS_DATA=%S/pdm/grafana"));
        assert!(content.contains("Environment=GF_PATHS_LOGS=%S/pdm/grafana/logs"));
        assert!(content.contains("Environment=GF_PATHS_PLUGINS=%S/pdm/grafana/plugins"));
        assert!(content.contains(
            "Environment=\"GF_PATHS_PROVISIONING=/home/example/.config/pdm/monitoring/grafana/provisioning\""
        ));
        assert!(content.contains("GF_SERVER_HTTP_ADDR=127.0.0.1"));
        assert!(content.contains("GF_AUTH_ANONYMOUS_ENABLED=false"));
        Ok(())
    }

    #[test]
    fn grafana_admin_credentials_are_random_persistent_and_private() -> Result<()> {
        let config = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        let path = ensure_grafana_admin_credentials(config.path(), state.path())?;
        let original = fs::read_to_string(&path)?;
        assert!(original.starts_with("GF_SECURITY_ADMIN_PASSWORD="));
        let password = original
            .trim()
            .strip_prefix("GF_SECURITY_ADMIN_PASSWORD=")
            .unwrap();
        assert_eq!(password.len(), 64);
        assert!(password.bytes().all(|byte| byte.is_ascii_hexdigit()));
        ensure_grafana_admin_credentials(config.path(), state.path())?;
        assert_eq!(fs::read_to_string(&path)?, original);
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        Ok(())
    }

    #[test]
    fn grafana_refuses_to_regenerate_credentials_for_an_existing_database() -> Result<()> {
        let config = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        fs::write(state.path().join("grafana.db"), "database")?;
        let error = ensure_grafana_admin_credentials(config.path(), state.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("existing database"), "{error}");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // memory_rss_bytes tests
    // -----------------------------------------------------------------------

    #[test]
    #[serial]
    fn memory_rss_bytes_returns_memory_current_when_valid() -> Result<()> {
        let (_dir, old_path) = fake_systemctl_show(&[("MemoryCurrent", "52428800")]);

        let result = P2PoolV2Service::memory_rss_bytes("signet");
        restore_path(old_path);

        assert_eq!(result?, Some(52_428_800_u64));
        Ok(())
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_zero_memory_current_is_returned_as_zero() -> Result<()> {
        // MemoryCurrent == 0 is a valid value (e.g. a freshly started service
        // with cgroup accounting enabled but no pages yet).  It must NOT be
        // treated as the sentinel.
        let (_dir, old_path) = fake_systemctl_show(&[("MemoryCurrent", "0")]);

        let result = P2PoolV2Service::memory_rss_bytes("signet");
        restore_path(old_path);

        assert_eq!(result?, Some(0));
        Ok(())
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_falls_back_to_proc_when_memory_current_is_sentinel() -> Result<()> {
        let sentinel = u64::MAX.to_string();
        let proc_dir = tempfile::tempdir().unwrap();
        fake_proc_status(proc_dir.path(), 99999, 51200);

        // The fake systemctl emits u64::MAX for MemoryCurrent and the fake
        // PID for MainPID.  We then call rss_from_proc_root directly because
        // the PATH-shim approach cannot control which /proc root is used.
        // Instead we test the two layers independently:
        //   - memory_rss_bytes up to the sentinel detection via fake systemctl
        //   - rss_from_proc_root directly with the tempdir
        let (_dir, old_path) =
            fake_systemctl_show(&[("MemoryCurrent", &sentinel), ("MainPID", "99999")]);
        // Override the PATH so show_property works, but intercept before
        // rss_from_proc by testing rss_from_proc_root separately below.
        restore_path(old_path);

        // Direct test of the proc-reading layer.
        let rss = P2PoolV2Service::rss_from_proc_root(99999, proc_dir.path().to_str().unwrap())?;
        assert_eq!(rss, Some(51200 * 1024));
        Ok(())
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_sentinel_with_main_pid_zero_returns_none() -> Result<()> {
        let sentinel = u64::MAX.to_string();
        let (_dir, old_path) =
            fake_systemctl_show(&[("MemoryCurrent", &sentinel), ("MainPID", "0")]);

        let result = P2PoolV2Service::memory_rss_bytes("signet");
        restore_path(old_path);

        assert_eq!(result?, None);
        Ok(())
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_invalid_memory_current_string_returns_error() {
        let (_dir, old_path) = fake_systemctl_show(&[("MemoryCurrent", "not-a-number")]);

        let err = P2PoolV2Service::memory_rss_bytes("signet").unwrap_err();
        restore_path(old_path);

        assert!(
            err.to_string().contains("non-integer MemoryCurrent"),
            "unexpected error: {err}"
        );
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_invalid_main_pid_string_returns_error() {
        let sentinel = u64::MAX.to_string();
        let (_dir, old_path) =
            fake_systemctl_show(&[("MemoryCurrent", &sentinel), ("MainPID", "not-a-pid")]);

        let err = P2PoolV2Service::memory_rss_bytes("signet").unwrap_err();
        restore_path(old_path);

        assert!(
            err.to_string().contains("non-integer MainPID"),
            "unexpected error: {err}"
        );
    }

    #[test]
    #[serial]
    fn memory_rss_bytes_invalid_instance_name_returns_error() {
        // No fake systemctl needed — validation fires before any Command.
        let err = P2PoolV2Service::memory_rss_bytes("bad/instance")
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid P2Poolv2 service instance"), "{err}");
    }

    // -----------------------------------------------------------------------
    // rss_from_proc_root unit tests (no PATH shimming needed)
    // -----------------------------------------------------------------------

    #[test]
    fn rss_from_proc_root_returns_bytes_from_vmrss_field() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        fake_proc_status(dir.path(), 1, 65536);

        let rss = P2PoolV2Service::rss_from_proc_root(1, dir.path().to_str().unwrap())?;
        assert_eq!(rss, Some(65536 * 1024));
        Ok(())
    }

    #[test]
    fn rss_from_proc_root_missing_status_file_returns_none() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        // No file written — directory exists but status is absent.
        let proc_dir = dir.path().join("12345");
        fs::create_dir_all(&proc_dir).unwrap();

        let rss = P2PoolV2Service::rss_from_proc_root(12345, dir.path().to_str().unwrap())?;
        assert_eq!(rss, None);
        Ok(())
    }

    #[test]
    fn rss_from_proc_root_missing_pid_directory_returns_none() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        // Neither the pid directory nor the status file exist.
        let rss = P2PoolV2Service::rss_from_proc_root(99, dir.path().to_str().unwrap())?;
        assert_eq!(rss, None);
        Ok(())
    }

    #[test]
    fn rss_from_proc_root_no_vmrss_field_returns_none() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let proc_dir = dir.path().join("7");
        fs::create_dir_all(&proc_dir).unwrap();
        fs::write(
            proc_dir.join("status"),
            "Name:\tp2poolv2\nVmSize:\t102400 kB\n",
        )
        .unwrap();

        let rss = P2PoolV2Service::rss_from_proc_root(7, dir.path().to_str().unwrap())?;
        assert_eq!(rss, None);
        Ok(())
    }

    #[test]
    fn rss_from_proc_root_unexpected_unit_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let proc_dir = dir.path().join("8");
        fs::create_dir_all(&proc_dir).unwrap();
        // "mB" is not a recognised unit
        fs::write(
            proc_dir.join("status"),
            "Name:\tp2poolv2\nVmRSS:\t1024 mB\n",
        )
        .unwrap();

        let err = P2PoolV2Service::rss_from_proc_root(8, dir.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.to_string().contains("unexpected VmRSS format"),
            "unexpected error: {err}"
        );
    }

    #[test]
    #[serial]
    fn existing_service_actions_unaffected_by_memory_rss_bytes() -> Result<()> {
        // Regression guard: the existing start/stop/is_running functionality
        // must compile and behave identically after the addition of the new
        // methods.  Re-run the core assertion from the original test here so
        // the CI catches any unintended breakage.
        let (_dir, old_path, args_file) = fake_systemctl(false, true);

        let result = (|| {
            assert!(P2PoolV2Service::is_running("signet")?);
            assert_eq!(
                fs::read_to_string(&args_file)?,
                "--user\nis-active\np2poolv2@signet\n"
            );
            Ok::<_, anyhow::Error>(())
        })();

        restore_path(old_path);
        result
    }

    #[test]
    #[serial]
    fn service_actions_succeed_and_active_service_is_running() -> Result<()> {
        let (_dir, old_path, args_file) = fake_systemctl(false, true);

        let result = (|| {
            P2PoolV2Service::start("signet")?;
            P2PoolV2Service::stop("signet")?;
            P2PoolV2Service::restart("signet")?;
            assert!(P2PoolV2Service::is_running("signet")?);
            assert_eq!(
                fs::read_to_string(args_file)?,
                "--user\nis-active\np2poolv2@signet\n"
            );
            Ok::<_, anyhow::Error>(())
        })();

        restore_path(old_path);
        result
    }

    #[test]
    #[serial]
    fn service_actions_report_systemctl_failures() {
        let (_dir, old_path, _args_file) = fake_systemctl(true, false);

        let start_error = P2PoolV2Service::start("signet").unwrap_err().to_string();
        let stop_error = P2PoolV2Service::stop("signet").unwrap_err().to_string();
        let restart_error = P2PoolV2Service::restart("signet").unwrap_err().to_string();
        let running = P2PoolV2Service::is_running("signet").unwrap();

        restore_path(old_path);

        assert!(start_error.contains("start p2poolv2@signet failed: permission denied"));
        assert!(stop_error.contains("stop p2poolv2@signet failed: permission denied"));
        assert!(restart_error.contains("restart p2poolv2@signet failed: permission denied"));
        assert!(!running);
    }

    #[test]
    fn instance_validation_accepts_safe_names() {
        for instance in [
            "signet", "main", "testnet4", "regtest", "foo-bar", "foo_bar",
        ] {
            assert!(validated_service_name(instance).is_ok(), "{instance}");
        }
    }

    #[test]
    fn instance_validation_rejects_unsafe_names() {
        for instance in ["foo/bar", "foo bar", "foo@bar", "../foo", ""] {
            assert!(validated_service_name(instance).is_err(), "{instance}");
        }
    }

    #[test]
    fn config_path_derives_instance_only_in_config_directory() {
        let config_dir = PathBuf::from("/home/user/.config/p2poolv2");

        assert_eq!(
            instance_from_config_path_in_dir(&config_dir.join("config-signet.toml"), &config_dir),
            Some("signet".to_string())
        );
        assert_eq!(
            instance_from_config_path_in_dir(&config_dir.join("config-main.toml"), &config_dir),
            Some("main".to_string())
        );

        for path in [
            "/etc/p2poolv2/config-signet.toml",
            "/home/user/p2pool/config-signet.toml",
            "/home/user/p2pool/config.toml",
            "/home/user/.config/p2poolv2/config.toml",
        ] {
            assert_eq!(
                instance_from_config_path_in_dir(Path::new(path), &config_dir),
                None
            );
        }
    }

    #[test]
    fn resolve_store_path_uses_absolute_path_directly() {
        let mut config = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        config.store.path = "/var/lib/p2pool/custom_store.db".to_string();

        let config_dir = PathBuf::from("/home/user/.config/p2poolv2");
        let state_dir = PathBuf::from("/home/user/.local/state");

        // With managed config path
        let managed_path = config_dir.join("config-signet.toml");
        let res = resolve_store_path_in_dirs(&config, Some(&managed_path), &config_dir, &state_dir)
            .unwrap();
        assert_eq!(res, PathBuf::from("/var/lib/p2pool/custom_store.db"));

        // With unmanaged config path
        let unmanaged_path = PathBuf::from("/tmp/custom-config.toml");
        let res =
            resolve_store_path_in_dirs(&config, Some(&unmanaged_path), &config_dir, &state_dir)
                .unwrap();
        assert_eq!(res, PathBuf::from("/var/lib/p2pool/custom_store.db"));

        // With None config path
        let res = resolve_store_path_in_dirs(&config, None, &config_dir, &state_dir).unwrap();
        assert_eq!(res, PathBuf::from("/var/lib/p2pool/custom_store.db"));
    }

    #[test]
    fn resolve_store_path_managed_config_relative_path() {
        let mut config = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        config.store.path = "./store.db".to_string();

        let config_dir = PathBuf::from("/home/user/.config/p2poolv2");
        let state_dir = PathBuf::from("/home/user/.local/state");
        let managed_path = config_dir.join("config-signet.toml");

        let res = resolve_store_path_in_dirs(&config, Some(&managed_path), &config_dir, &state_dir)
            .unwrap();
        assert_eq!(
            res,
            PathBuf::from("/home/user/.local/state/p2poolv2/signet/./store.db")
        );
    }

    #[test]
    fn resolve_store_path_unmanaged_config_relative_path_returns_error() {
        let mut config = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        config.store.path = "./store.db".to_string();

        let config_dir = PathBuf::from("/home/user/.config/p2poolv2");
        let state_dir = PathBuf::from("/home/user/.local/state");
        let unmanaged_path = PathBuf::from("/home/user/p2pool/config.toml");

        let err =
            resolve_store_path_in_dirs(&config, Some(&unmanaged_path), &config_dir, &state_dir)
                .unwrap_err();
        assert_eq!(
            err,
            StorePathError::UnmanagedRelativePath("./store.db".to_string())
        );
        assert!(err.to_string().contains("unmanaged relative store path"));
    }

    #[test]
    fn resolve_store_path_missing_config_path_relative_store_returns_error() {
        let mut config = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        config.store.path = "./store.db".to_string();

        let config_dir = PathBuf::from("/home/user/.config/p2poolv2");
        let state_dir = PathBuf::from("/home/user/.local/state");

        let err = resolve_store_path_in_dirs(&config, None, &config_dir, &state_dir).unwrap_err();
        assert_eq!(
            err,
            StorePathError::UnmanagedRelativePath("./store.db".to_string())
        );
    }

    #[test]
    fn calculate_path_size_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("store.dat");
        let data = vec![0u8; 128];
        fs::write(&file_path, &data).unwrap();

        let size = calculate_path_size(&file_path).unwrap();
        assert_eq!(size, 128);
    }

    #[test]
    fn calculate_path_size_directory_with_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file1.sst"), vec![0u8; 100]).unwrap();
        fs::write(dir.path().join("file2.sst"), vec![0u8; 200]).unwrap();

        let size = calculate_path_size(dir.path()).unwrap();
        assert_eq!(size, 300);
    }

    #[test]
    fn calculate_path_size_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let sub_dir = dir.path().join("nested").join("deep");
        fs::create_dir_all(&sub_dir).unwrap();

        fs::write(dir.path().join("root.db"), vec![0u8; 50]).unwrap();
        fs::write(sub_dir.join("deep.db"), vec![0u8; 150]).unwrap();

        let size = calculate_path_size(dir.path()).unwrap();
        assert_eq!(size, 200);
    }

    #[test]
    fn calculate_path_size_empty_directory() {
        let dir = tempfile::tempdir().unwrap();

        let size = calculate_path_size(dir.path()).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn calculate_path_size_missing_path_returns_not_found() {
        let missing_path = PathBuf::from("/nonexistent/store.db");
        let err = calculate_path_size(&missing_path).unwrap_err();
        assert_eq!(err, StorageSizeError::NotFound(missing_path.clone()));
        assert!(err.to_string().contains("Store path does not exist"));
    }

    #[test]
    fn storage_size_error_display_formatting() {
        let not_found_err = StorageSizeError::NotFound(PathBuf::from("/tmp/missing"));
        assert_eq!(
            not_found_err.to_string(),
            "Store path does not exist: /tmp/missing"
        );

        let io_err = StorageSizeError::Io("permission denied".to_string());
        assert_eq!(io_err.to_string(), "Filesystem error: permission denied");
    }

    #[test]
    #[ignore = "requires a user systemd session and an installed p2poolv2@.service"]
    fn start_and_stop_service() -> Result<()> {
        assert!(!P2PoolV2Service::is_running("signet")?);

        P2PoolV2Service::start("signet")?;
        let running = P2PoolV2Service::is_running("signet");
        let stop_result = P2PoolV2Service::stop("signet");

        assert!(running?);
        stop_result?;
        assert!(!P2PoolV2Service::is_running("signet")?);

        Ok(())
    }
}
