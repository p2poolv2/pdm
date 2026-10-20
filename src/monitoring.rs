// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Monitoring configuration generation for PDM.
//!
//! Generates a Prometheus scrape configuration and Grafana provisioning
//! files for the P2Pool v2 metrics endpoint.
//!
//! ## Authentication
//!
//! P2Pool v2's HTTP API supports optional HTTP Basic Auth controlled by
//! `api.auth_user` and `api.auth_password` in the P2Pool config file.
//! The `api.auth_token` field is an internal server-side HMAC token and
//! is **not** a plaintext password; Prometheus cannot use it.
//!
//! When both `auth_user` and `auth_password` are set, this module writes
//! a `basic_auth` block into the generated `prometheus.yml`.  The output
//! file is created with mode 0600 so that the password is not readable
//! by other users on the system.  The password is never written to logs
//! or displayed in the TUI.
//!
//! When authentication is not configured, the `basic_auth` block is
//! omitted entirely.

use anyhow::{Context, Result};
use p2poolv2_config::Config as P2PoolConfig;
use std::path::{Path, PathBuf};

use crate::settings;

// ---------------------------------------------------------------------------
// Directory / path helpers
// ---------------------------------------------------------------------------

/// Returns the directory used for monitoring configuration files.
///
/// The directory is `<pdm_config_dir>/monitoring/`.
///
/// # Errors
/// Propagates any error from [`settings::config_dir`].
pub fn monitoring_config_dir() -> Result<PathBuf> {
    Ok(settings::config_dir()?.join("monitoring"))
}

/// Returns the path where `prometheus.yml` will be written.
///
/// # Errors
/// Propagates any error from [`monitoring_config_dir`].
pub fn prometheus_yml_path() -> Result<PathBuf> {
    Ok(monitoring_config_dir()?.join("prometheus.yml"))
}

/// Grafana's default local Prometheus endpoint.
pub const DEFAULT_PROMETHEUS_URL: &str = "http://127.0.0.1:9090";

const POOL_DASHBOARD: &str = include_str!("../assets/grafana/pool.json");
const USERS_DASHBOARD: &str = include_str!("../assets/grafana/users.json");

/// Files written for Grafana provisioning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrafanaProvisioningPaths {
    pub datasource: PathBuf,
    pub dashboard_provider: PathBuf,
    pub dashboards: Vec<PathBuf>,
}

/// All monitoring configuration files written by PDM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitoringOutputPaths {
    pub prometheus: PathBuf,
    pub grafana: GrafanaProvisioningPaths,
}

/// Produces Grafana datasource provisioning YAML for the Prometheus URL.
#[must_use]
pub fn generate_grafana_datasource_yaml(prometheus_url: &str) -> String {
    format!(
        "apiVersion: 1\n\ndatasources:\n  - name: Prometheus\n    type: prometheus\n    access: proxy\n    url: {}\n    isDefault: true\n    editable: true\n    jsonData:\n      httpMethod: POST\n      timeInterval: 5s\n",
        yaml_quote(prometheus_url)
    )
}

/// Produces a file provider configuration pointing at PDM's dashboard assets.
#[must_use]
pub fn generate_grafana_dashboard_provider_yaml(dashboards_dir: &Path) -> String {
    format!(
        "apiVersion: 1\n\nproviders:\n  - name: 'P2Poolv2'\n    orgId: 1\n    folder: ''\n    type: file\n    disableDeletion: true\n    updateIntervalSeconds: 10\n    allowUiUpdates: false\n    options:\n      path: {}\n",
        yaml_quote(&dashboards_dir.to_string_lossy())
    )
}

/// Generates P2Pool v2's public Grafana dashboards and provisioning files.
/// The upstream private `users_private.json` variant is intentionally excluded.
pub fn write_grafana_provisioning(prometheus_url: &str) -> Result<GrafanaProvisioningPaths> {
    let root = monitoring_config_dir()?.join("grafana");
    write_grafana_provisioning_in(&root, prometheus_url)
}

fn write_grafana_provisioning_in(
    root: &Path,
    prometheus_url: &str,
) -> Result<GrafanaProvisioningPaths> {
    let datasource_dir = root.join("provisioning").join("datasources");
    let provider_dir = root.join("provisioning").join("dashboards");
    let dashboards_dir = root.join("dashboards");
    for dir in [&datasource_dir, &provider_dir, &dashboards_dir] {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create Grafana directory: {}", dir.display()))?;
    }

    let datasource = datasource_dir.join("prometheus.yml");
    std::fs::write(
        &datasource,
        generate_grafana_datasource_yaml(prometheus_url),
    )
    .with_context(|| {
        format!(
            "could not write Grafana datasource: {}",
            datasource.display()
        )
    })?;

    let dashboard_provider = provider_dir.join("dashboards.yml");
    std::fs::write(
        &dashboard_provider,
        generate_grafana_dashboard_provider_yaml(&dashboards_dir),
    )
    .with_context(|| {
        format!(
            "could not write Grafana dashboard provider: {}",
            dashboard_provider.display()
        )
    })?;

    let pool = dashboards_dir.join("pool.json");
    std::fs::write(&pool, POOL_DASHBOARD)
        .with_context(|| format!("could not write upstream dashboard: {}", pool.display()))?;
    let users = dashboards_dir.join("users.json");
    std::fs::write(&users, USERS_DASHBOARD)
        .with_context(|| format!("could not write upstream dashboard: {}", users.display()))?;

    Ok(GrafanaProvisioningPaths {
        datasource,
        dashboard_provider,
        dashboards: vec![pool, users],
    })
}

/// Writes Prometheus and Grafana configuration files under PDM's monitoring
/// directory.
pub fn write_monitoring_configs(cfg: &P2PoolConfig) -> Result<MonitoringOutputPaths> {
    let prometheus = write_prometheus_config(cfg)?;
    let grafana = write_grafana_provisioning(DEFAULT_PROMETHEUS_URL)?;
    Ok(MonitoringOutputPaths {
        prometheus,
        grafana,
    })
}

// ---------------------------------------------------------------------------
// YAML generation
// ---------------------------------------------------------------------------

/// Produces valid Prometheus YAML for the given P2Pool v2 configuration.
///
/// The generated YAML targets `<cfg.api.hostname>:<cfg.api.port>`.
/// If `cfg.api.auth_user` and `cfg.api.auth_password` are both set,
/// a `basic_auth` block is included.
///
/// Values are escaped to prevent YAML injection.  In practice the
/// hostname and port come from a structured config value (a `String`
/// and `u16`) and are therefore safe, but we still quote the target
/// string explicitly.
#[must_use]
pub fn generate_prometheus_yaml(cfg: &P2PoolConfig) -> String {
    let target = format!("{}:{}", cfg.api.hostname, cfg.api.port);

    // Resolve optional basic-auth credentials.
    // auth_token is NOT a password; only auth_user + auth_password is
    // usable by Prometheus basic_auth.
    let auth_block = match (&cfg.api.auth_user, &cfg.api.auth_password) {
        (Some(user), Some(password)) if !user.is_empty() && !password.is_empty() => {
            format!(
                "    basic_auth:\n      username: {}\n      password: {}\n",
                yaml_quote(user),
                yaml_quote(password),
            )
        }
        _ => String::new(),
    };

    format!(
        r#"global:
  scrape_interval: 15s

scrape_configs:
  - job_name: p2poolv2
    scrape_interval: 5s
    metrics_path: /metrics
{auth_block}    static_configs:
      - targets:
          - {target_quoted}
"#,
        auth_block = auth_block,
        target_quoted = yaml_quote(&target),
    )
}

/// Wraps a string value in YAML single quotes, escaping any embedded
/// single quotes by doubling them (`'` → `''`).
///
/// Using single-quoted YAML scalars avoids the need to worry about
/// backslash escapes; the only special character is `'` itself.
fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

// ---------------------------------------------------------------------------
// Write to disk
// ---------------------------------------------------------------------------

/// Generates `prometheus.yml` from `cfg` and writes it to the
/// monitoring configuration directory.
///
/// The directory is created if it does not already exist.  The file is
/// written atomically: a temporary file is written first, then renamed
/// into place so that an interrupted write never leaves a truncated
/// config.
///
/// On Unix the file is created with mode 0600 (owner read/write only)
/// when the config contains credentials.  On other platforms the file
/// permissions are left to the OS default.
///
/// # Returns
/// The path of the written file on success.
///
/// # Errors
/// Returns an error if the directory cannot be created, the file cannot
/// be written, or the rename fails.
pub fn write_prometheus_config(cfg: &P2PoolConfig) -> Result<PathBuf> {
    let dir = monitoring_config_dir().context("could not resolve monitoring config directory")?;
    std::fs::create_dir_all(&dir).with_context(|| {
        format!(
            "could not create monitoring config directory: {}",
            dir.display()
        )
    })?;

    let out_path = dir.join("prometheus.yml");
    let content = generate_prometheus_yaml(cfg);

    // Write via a temporary file in the same directory so that the
    // rename is atomic on POSIX systems.
    let tmp_path = dir.join(".prometheus.yml.tmp");
    write_file_safe(&tmp_path, &content, cfg).with_context(|| {
        format!(
            "could not write temporary prometheus config: {}",
            tmp_path.display()
        )
    })?;
    std::fs::rename(&tmp_path, &out_path).with_context(|| {
        format!(
            "could not rename {} to {}",
            tmp_path.display(),
            out_path.display()
        )
    })?;

    Ok(out_path)
}

/// Writes `content` to `path` and sets restrictive permissions when the
/// config contains credentials.
fn write_file_safe(path: &Path, content: &str, cfg: &P2PoolConfig) -> Result<()> {
    let has_credentials = cfg.api.auth_user.as_ref().is_some_and(|u| !u.is_empty())
        && cfg
            .api
            .auth_password
            .as_ref()
            .is_some_and(|p| !p.is_empty());

    std::fs::write(path, content)
        .with_context(|| format!("could not write file: {}", path.display()))?;

    #[cfg(unix)]
    if has_credentials {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("could not set permissions on: {}", path.display()))?;
    }

    // On non-Unix we still succeed but cannot enforce 0600.
    #[cfg(not(unix))]
    let _ = has_credentials;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn make_config_with_api(hostname: &str, port: u16) -> P2PoolConfig {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p2pool.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[network]
listen_address = "/ip4/127.0.0.1/tcp/6884"
dial_peers = []
max_pending_incoming = 10
max_pending_outgoing = 10
max_established_incoming = 50
max_established_outgoing = 50
max_established_per_peer = 1
max_workbase_per_second = 10
max_userworkbase_per_second = 10
max_miningshare_per_second = 100
max_inventory_per_second = 100
max_transaction_per_second = 100
max_requests_per_second = 100
dial_timeout_secs = 30

[store]
path = "./store.db"
background_task_frequency_hours = 24
pplns_ttl_days = 7

[stratum]
hostname = "pool.example.com"
port = 3333
start_difficulty = 10000
minimum_difficulty = 100
solo_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
bootstrap_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
zmqpubhashblock = "tcp://127.0.0.1:28332"
network = "signet"
version_mask = "1fffe000"
difficulty_multiplier = 1.0
pool_signature = "P2Poolv2"

[bitcoinrpc]
url = "http://127.0.0.1:38332"
username = "p2pool"
password = "p2pool"

[logging]
file = "./logs/p2pool.log"
console = true
level = "info"
stats_dir = "./logs/stats"

[api]
hostname = "{hostname}"
port = {port}
"#
            ),
        )
        .unwrap();
        // keep dir alive until Config is loaded
        let cfg =
            P2PoolConfig::load(path.to_str().unwrap()).expect("inline test config must parse");
        drop(dir);
        cfg
    }

    fn make_config_with_auth(
        hostname: &str,
        port: u16,
        auth_user: Option<&str>,
        auth_password: Option<&str>,
    ) -> P2PoolConfig {
        let mut cfg = make_config_with_api(hostname, port);
        cfg.api.auth_user = auth_user.map(str::to_string);
        cfg.api.auth_password = auth_password.map(str::to_string);
        cfg
    }

    fn set_config_dir(dir: &TempDir) {
        // SAFETY: Only called from #[serial] tests; PDM_CONFIG_DIR is
        // not read concurrently while the serial lock is held.
        unsafe { std::env::set_var("PDM_CONFIG_DIR", dir.path()) };
    }

    // ------------------------------------------------------------------
    // generate_prometheus_yaml
    // ------------------------------------------------------------------

    #[test]
    fn default_hostname_and_port_appear_in_target() {
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("'127.0.0.1:46884'"), "yaml:\n{yaml}");
    }

    #[test]
    fn custom_hostname_and_port_appear_in_target() {
        let cfg = make_config_with_api("192.0.2.10", 9090);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("'192.0.2.10:9090'"), "yaml:\n{yaml}");
    }

    #[test]
    fn metrics_path_is_slash_metrics() {
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("metrics_path: /metrics"), "yaml:\n{yaml}");
    }

    #[test]
    fn scrape_interval_is_five_seconds() {
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("scrape_interval: 5s"), "yaml:\n{yaml}");
    }

    #[test]
    fn job_name_is_p2poolv2() {
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("job_name: p2poolv2"), "yaml:\n{yaml}");
    }

    #[test]
    fn auth_disabled_when_neither_user_nor_password_set() {
        let cfg = make_config_with_auth("127.0.0.1", 46884, None, None);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(
            !yaml.contains("basic_auth"),
            "basic_auth must not appear when auth is unset; yaml:\n{yaml}"
        );
    }

    #[test]
    fn auth_disabled_when_only_user_set() {
        let cfg = make_config_with_auth("127.0.0.1", 46884, Some("alice"), None);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(
            !yaml.contains("basic_auth"),
            "basic_auth must not appear without password; yaml:\n{yaml}"
        );
    }

    #[test]
    fn auth_disabled_when_only_password_set() {
        let cfg = make_config_with_auth("127.0.0.1", 46884, None, Some("secret"));
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(
            !yaml.contains("basic_auth"),
            "basic_auth must not appear without user; yaml:\n{yaml}"
        );
    }

    #[test]
    fn auth_enabled_when_user_and_password_both_set() {
        let cfg = make_config_with_auth("127.0.0.1", 46884, Some("alice"), Some("s3cr3t"));
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("basic_auth"), "yaml:\n{yaml}");
        assert!(yaml.contains("username: 'alice'"), "yaml:\n{yaml}");
        assert!(yaml.contains("password: 's3cr3t'"), "yaml:\n{yaml}");
    }

    #[test]
    fn auth_token_field_is_not_used_as_password() {
        // auth_token is an internal server-side HMAC value and must
        // never appear in the generated Prometheus config.
        let mut cfg = make_config_with_auth("127.0.0.1", 46884, Some("alice"), None);
        cfg.api.auth_token = Some("hmac-salt$token".to_string());
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(
            !yaml.contains("hmac-salt"),
            "auth_token must not appear in yaml:\n{yaml}"
        );
        assert!(
            !yaml.contains("basic_auth"),
            "basic_auth must not appear without auth_password; yaml:\n{yaml}"
        );
    }

    #[test]
    fn yaml_quote_escapes_single_quotes() {
        assert_eq!(yaml_quote("it's"), "'it''s'");
        assert_eq!(yaml_quote("plain"), "'plain'");
        assert_eq!(yaml_quote("a'b'c"), "'a''b''c'");
    }

    #[test]
    fn hostname_with_special_chars_is_safely_quoted() {
        // Hostname containing single quotes would be malformed; we still
        // produce valid YAML by doubling them.
        let mut cfg = make_config_with_api("127.0.0.1", 46884);
        cfg.api.hostname = "host'name".to_string();
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("'host''name:46884'"), "yaml:\n{yaml}");
    }

    // ------------------------------------------------------------------
    // write_prometheus_config (filesystem)
    // ------------------------------------------------------------------

    #[test]
    #[serial_test::serial]
    fn write_creates_output_directory_and_file() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);

        let cfg = make_config_with_api("127.0.0.1", 46884);
        let path = write_prometheus_config(&cfg).unwrap();

        assert!(
            path.exists(),
            "prometheus.yml must exist at {}",
            path.display()
        );
        assert_eq!(path, dir.path().join("monitoring").join("prometheus.yml"));
    }

    #[test]
    #[serial_test::serial]
    fn write_file_contains_correct_target() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);

        let cfg = make_config_with_api("10.0.0.1", 9999);
        let path = write_prometheus_config(&cfg).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("'10.0.0.1:9999'"), "content:\n{content}");
    }

    #[test]
    #[serial_test::serial]
    fn write_is_idempotent_and_overwrites_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);

        let cfg1 = make_config_with_api("127.0.0.1", 1111);
        write_prometheus_config(&cfg1).unwrap();

        let cfg2 = make_config_with_api("192.168.1.1", 2222);
        let path = write_prometheus_config(&cfg2).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("'192.168.1.1:2222'"),
            "content:\n{content}"
        );
        assert!(
            !content.contains("1111"),
            "old config must be overwritten; content:\n{content}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn write_fails_when_directory_cannot_be_created() {
        let dir = tempfile::tempdir().unwrap();
        // Point to a file path so create_dir_all fails.
        let blocker = dir.path().join("monitoring");
        std::fs::write(&blocker, "occupied").unwrap();
        set_config_dir(&dir);

        // Now monitoring/ is a *file*, so creating the subdirectory must fail.
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let result = write_prometheus_config(&cfg);
        assert!(
            result.is_err(),
            "expected error when directory cannot be created"
        );
    }

    #[test]
    #[serial_test::serial]
    fn write_with_auth_does_not_appear_in_content_for_unauthenticated_config() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);

        let cfg = make_config_with_auth("127.0.0.1", 46884, None, None);
        let path = write_prometheus_config(&cfg).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("basic_auth"),
            "basic_auth must not appear for unauthenticated config; content:\n{content}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn output_path_is_under_pdm_config_monitoring_subdir() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);

        let cfg = make_config_with_api("127.0.0.1", 46884);
        let path = write_prometheus_config(&cfg).unwrap();

        let expected = dir.path().join("monitoring").join("prometheus.yml");
        assert_eq!(path, expected);
    }

    // ------------------------------------------------------------------
    // Grafana provisioning
    // ------------------------------------------------------------------

    #[test]
    fn grafana_datasource_uses_configured_url_as_default() {
        let yaml = generate_grafana_datasource_yaml("http://192.0.2.4:9191");
        assert!(yaml.contains("name: Prometheus"));
        assert!(yaml.contains("type: prometheus"));
        assert!(yaml.contains("url: 'http://192.0.2.4:9191'"));
        assert!(yaml.contains("isDefault: true"));
        assert!(!yaml.contains("password"));
    }

    #[test]
    fn dashboard_provider_uses_generated_assets_directory() {
        let path = Path::new("/home/example/.config/pdm/monitoring/grafana/dashboards");
        let yaml = generate_grafana_dashboard_provider_yaml(path);
        assert!(yaml.contains("name: 'P2Poolv2'"));
        assert!(yaml.contains("type: file"));
        assert!(yaml.contains("path: '/home/example/.config/pdm/monitoring/grafana/dashboards'"));
    }

    #[test]
    fn bundled_public_dashboard_assets_are_valid_and_private_variant_is_excluded() {
        let pool: serde_json::Value = serde_json::from_str(POOL_DASHBOARD).unwrap();
        let users: serde_json::Value = serde_json::from_str(USERS_DASHBOARD).unwrap();
        assert_eq!(pool["title"], "Pool");
        assert_eq!(users["title"], "Users and Workers");
        assert!(!POOL_DASHBOARD.contains("users_private"));
        assert!(!USERS_DASHBOARD.contains("users_private"));
    }

    #[test]
    fn grafana_provisioning_writes_datasource_provider_and_dashboard_assets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("grafana");
        let output = write_grafana_provisioning_in(&root, DEFAULT_PROMETHEUS_URL).unwrap();
        assert!(output.datasource.is_file());
        assert!(output.dashboard_provider.is_file());
        assert_eq!(output.dashboards.len(), 2);
        assert!(output.dashboards.iter().all(|path| path.is_file()));
        assert_eq!(
            std::fs::read_to_string(&output.dashboards[0]).unwrap(),
            POOL_DASHBOARD
        );
        assert_eq!(
            std::fs::read_to_string(&output.dashboards[1]).unwrap(),
            USERS_DASHBOARD
        );
        assert!(
            std::fs::read_to_string(&output.datasource)
                .unwrap()
                .contains(DEFAULT_PROMETHEUS_URL)
        );
        assert!(
            std::fs::read_to_string(&output.dashboard_provider)
                .unwrap()
                .contains(&root.join("dashboards").to_string_lossy().to_string())
        );
    }

    #[test]
    fn grafana_provisioning_reports_filesystem_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("grafana");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("provisioning"), "blocks directory creation").unwrap();
        let error = write_grafana_provisioning_in(&root, DEFAULT_PROMETHEUS_URL).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("could not create Grafana directory")
        );
    }
}
