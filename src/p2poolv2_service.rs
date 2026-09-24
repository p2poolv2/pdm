// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_PREFIX: &str = "p2poolv2@";

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
