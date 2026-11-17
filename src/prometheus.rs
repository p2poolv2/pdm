// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Prometheus configuration generation for PDM.
//!
//! Generates a Prometheus scrape configuration for the P2Pool v2 metrics
//! endpoint.
//!
//! ## Authentication
//!
//! P2Poolv2's HTTP API supports optional HTTP Basic Auth controlled by
//! `api.auth_user` and `api.auth_password` in the P2Pool config file.
//! The `api.auth_token` field is an internal server-side HMAC token and
//! is not a plaintext password; Prometheus cannot use it.
//!
//! When both `auth_user` and `auth_password` are set, this module writes
//! a `basic_auth` block into the generated `prometheus.yml`. The output
//! file is created with mode 0600 so that the password is not readable
//! by other users on the system.
//!
//! When authentication is not configured, the `basic_auth` block is
//! omitted entirely.

use anyhow::{Context, Result};
use p2poolv2_config::Config as P2PoolConfig;
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

use crate::settings;

/// Returns the directory used for Prometheus configuration files.
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

/// Produces valid Prometheus YAML for the given P2Poolv2 configuration.
///
/// The generated YAML targets `<cfg.api.hostname>:<cfg.api.port>`, with
/// IPv6 hostnames bracketed as required by Prometheus target syntax.
/// If `cfg.api.auth_user` and `cfg.api.auth_password` are both set,
/// a `basic_auth` block is included.
///
/// Values are escaped to prevent YAML injection. The hostname and port
/// come from a structured config value (a `String` and `u16`), but the
/// target string is explicitly quoted.
#[must_use]
pub fn generate_prometheus_yaml(cfg: &P2PoolConfig) -> String {
    let target = if cfg.api.hostname.parse::<Ipv6Addr>().is_ok() {
        format!("[{}]:{}", cfg.api.hostname, cfg.api.port)
    } else {
        format!("{}:{}", cfg.api.hostname, cfg.api.port)
    };

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

/// Wraps a string in a YAML double-quoted scalar, escaping special characters.
fn yaml_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\u{2028}' => quoted.push_str("\\u2028"),
            '\u{2029}' => quoted.push_str("\\u2029"),
            character if character.is_control() => {
                let codepoint = u32::from(character);
                if codepoint <= 0xff {
                    write!(quoted, "\\x{codepoint:02X}").expect("writing to a String cannot fail");
                } else if codepoint <= 0xffff {
                    write!(quoted, "\\u{codepoint:04X}").expect("writing to a String cannot fail");
                } else {
                    write!(quoted, "\\U{codepoint:08X}").expect("writing to a String cannot fail");
                }
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// Generates `prometheus.yml` from `cfg` and writes it to the
/// monitoring configuration directory.
///
/// The directory is created if it does not already exist. The file is
/// written atomically: a temporary file is written first, then renamed
/// into place so that an interrupted write never leaves a truncated
/// config.
///
/// On Unix the file is created with mode 0600 (owner read/write only)
/// when the config contains credentials. On other platforms the file
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

    let has_credentials = cfg.api.auth_user.as_ref().is_some_and(|u| !u.is_empty())
        && cfg
            .api
            .auth_password
            .as_ref()
            .is_some_and(|p| !p.is_empty());

    write_config_atomically(&dir, &out_path, &content, has_credentials)?;

    Ok(out_path)
}

fn write_config_atomically(
    dir: &Path,
    out_path: &Path,
    content: &str,
    _has_credentials: bool,
) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    if _has_credentials {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let (tmp_path, mut file) = loop {
        let unique_id = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!(
            ".prometheus.yml.{}.{}.tmp",
            std::process::id(),
            unique_id
        ));

        match options.open(&candidate) {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not create temporary config in {}", dir.display())
                });
            }
        }
    };

    let write_result = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);

    let result = write_result
        .context("could not write temporary Prometheus config")
        .and_then(|()| {
            std::fs::rename(&tmp_path, out_path).with_context(|| {
                format!(
                    "could not rename {} to {}",
                    tmp_path.display(),
                    out_path.display()
                )
            })
        });

    if let Err(error) = result {
        match std::fs::remove_file(&tmp_path) {
            Ok(()) => {}
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                return Err(error.context(format!(
                    "also failed to remove temporary config {}: {cleanup_error}",
                    tmp_path.display()
                )));
            }
        }
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

        // Keep directory alive until Config is loaded.
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

    // generate_prometheus_yaml
    #[test]
    fn default_hostname_and_port_appear_in_target() {
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("\"127.0.0.1:46884\""), "yaml:\n{yaml}");
    }

    #[test]
    fn custom_hostname_and_port_appear_in_target() {
        let cfg = make_config_with_api("192.0.2.10", 9090);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("\"192.0.2.10:9090\""), "yaml:\n{yaml}");
    }

    #[test]
    fn ipv6_hostname_is_bracketed_in_target() {
        let cfg = make_config_with_api("::1", 46884);
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("\"[::1]:46884\""), "yaml:\n{yaml}");
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
        assert!(yaml.contains("username: \"alice\""), "yaml:\n{yaml}");
        assert!(yaml.contains("password: \"s3cr3t\""), "yaml:\n{yaml}");
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
        assert_eq!(yaml_quote("it's"), "\"it's\"");
        assert_eq!(yaml_quote("plain"), "\"plain\"");
        assert_eq!(yaml_quote("a'b'c"), "\"a'b'c\"");
    }

    #[test]
    fn yaml_quote_escapes_quotes_backslashes_and_control_characters() {
        assert_eq!(
            yaml_quote("quote\" slash\\ newline\n tab\t control\u{1b}"),
            "\"quote\\\" slash\\\\ newline\\x0A tab\\x09 control\\x1B\""
        );
        assert_eq!(
            yaml_quote("line\u{2028}separator"),
            "\"line\\u2028separator\""
        );
    }

    #[test]
    fn credentials_with_control_characters_are_escaped_in_yaml() {
        let cfg = make_config_with_auth(
            "127.0.0.1",
            46884,
            Some("user\nname"),
            Some("pass\rword\u{7f}"),
        );
        let yaml = generate_prometheus_yaml(&cfg);
        assert!(
            yaml.contains("username: \"user\\x0Aname\""),
            "yaml:\n{yaml}"
        );
        assert!(
            yaml.contains("password: \"pass\\x0Dword\\x7F\""),
            "yaml:\n{yaml}"
        );
    }

    #[test]
    fn hostname_with_special_chars_is_safely_quoted() {
        // Hostname containing single quotes would be malformed; we still
        // produce valid YAML by doubling them.
        let mut cfg = make_config_with_api("127.0.0.1", 46884);
        cfg.api.hostname = "host'name".to_string();

        let yaml = generate_prometheus_yaml(&cfg);
        assert!(yaml.contains("\"host'name:46884\""), "yaml:\n{yaml}");
    }

    // write_prometheus_config (filesystem)
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
        assert!(content.contains("\"10.0.0.1:9999\""), "content:\n{content}");
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
            content.contains("\"192.168.1.1:2222\""),
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

        // Now monitoring/ is a file, so creating the subdirectory must fail.
        let cfg = make_config_with_api("127.0.0.1", 46884);
        let result = write_prometheus_config(&cfg);

        assert!(
            result.is_err(),
            "expected error when directory cannot be created"
        );
    }

    #[test]
    #[serial_test::serial]
    fn write_without_auth_omits_basic_auth() {
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

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn config_with_credentials_has_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("PDM_CONFIG_DIR", temp_dir.path());
        }

        let mut cfg = make_config_with_api("127.0.0.1", 46884);
        cfg.api.auth_user = Some("prometheus".to_string());
        cfg.api.auth_password = Some("secret".to_string());

        write_prometheus_config(&cfg).unwrap();

        let path = prometheus_yml_path().unwrap();
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;

        assert_eq!(mode, 0o600);
        unsafe {
            std::env::remove_var("PDM_CONFIG_DIR");
        }
    }

    #[test]
    fn rename_failure_cleans_up_temporary_config() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("prometheus.yml");
        std::fs::create_dir(&output_path).unwrap();
        std::fs::write(output_path.join("preserve"), "existing").unwrap();

        let result = write_config_atomically(dir.path(), &output_path, "new config", false);

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(output_path.join("preserve")).unwrap(),
            "existing"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
