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
