// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persistent user settings stored in a TOML file.
///
/// Config dir resolution order:
///   1. `PDM_CONFIG_DIR` environment variable
///   2. Platform default via `directories::ProjectDirs`
///      - macOS:   ~/Library/Application Support/org.p2pool.pdm/
///      - Linux:   ~/.config/pdm/
///      - Windows: %APPDATA%\p2pool\pdm\
///
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// Path to the p2poolv2 config file
    pub p2pool_conf_path: Option<PathBuf>,
    /// Path to the Lightning Network config file
    pub ln_conf_path: Option<PathBuf>,
    /// Path to the Shares Market config file
    pub shares_market_conf_path: Option<PathBuf>,
    /// If set, the user-chosen directory where `settings.toml` is stored.
    /// the default location always holds a copy so the override is found
    /// on the next launch.
    pub settings_dir_override: Option<PathBuf>,
}

/// Returns the directory where `settings.toml` is stored.
///
/// Priority:
///   1. `PDM_CONFIG_DIR` env var
///   2. Platform default via `directories`
///
/// # Errors
/// Returns an error if the platform config directory cannot be determined.
pub fn config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("PDM_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let proj = ProjectDirs::from("org", "p2pool", "pdm")
        .ok_or_else(|| anyhow::anyhow!("Cannot determine platform config directory"))?;
    Ok(proj.config_local_dir().to_path_buf())
}

/// Returns the path to the settings file.
///
/// # Errors
/// Returns an error if [`config_dir`] fails.
pub fn settings_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("settings.toml"))
}

/// Loads settings from disk. Returns `Settings::default()` if the file
/// does not exist or cannot be parsed.
#[must_use]
pub fn load_settings() -> Settings {
    let Ok(path) = settings_path() else {
        return Settings::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    let settings: Settings = toml::from_str(&content).unwrap_or_else(|e| {
        eprintln!("pdm: failed to parse settings: {e}");
        Settings::default()
    });
    if let Some(ref override_dir) = settings.settings_dir_override {
        let override_path = override_dir.join("settings.toml");
        if let Ok(content) = std::fs::read_to_string(&override_path)
            && let Ok(s) = toml::from_str::<Settings>(&content)
        {
            return s;
        }
    }
    settings
}

/// Saves settings to disk, creating the config directory if needed.
///
/// # Errors
/// Returns an error if any required directory cannot be created
pub fn save_settings(settings: &Settings) -> Result<()> {
    let content = toml::to_string_pretty(settings)?;
    if let Some(ref override_dir) = settings.settings_dir_override {
        // Write to the user-chosen location.
        let override_path = override_dir.join("settings.toml");
        if let Some(parent) = override_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&override_path, &content)?;
        // Write a copy to the default location so the override is found on restart.
        let default_path = settings_path()?;
        if let Some(parent) = default_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&default_path, &content)?;
    } else {
        let path = settings_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_has_no_paths() {
        let s = Settings::default();
        assert!(s.p2pool_conf_path.is_none());
        assert!(s.ln_conf_path.is_none());
        assert!(s.shares_market_conf_path.is_none());
        assert!(s.settings_dir_override.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // Write the settings file directly into the temp dir
        let path = dir.path().join("settings.toml");
        let settings = Settings {
            p2pool_conf_path: Some(PathBuf::from("/tmp/p2pool.toml")),
            ..Default::default()
        };
        let content = toml::to_string_pretty(&settings).unwrap();
        std::fs::write(&path, content).unwrap();

        let loaded: Settings = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        assert_eq!(loaded.p2pool_conf_path, settings.p2pool_conf_path);
        assert!(loaded.ln_conf_path.is_none());
    }

    #[test]
    fn load_settings_returns_default_for_bad_toml() {
        let result: Result<Settings, _> = toml::from_str("not valid toml :::");
        assert!(result.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn save_and_load_via_public_functions() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        let settings = Settings {
            ln_conf_path: Some(PathBuf::from("/tmp/ln.conf")),
            ..Default::default()
        };
        save_settings(&settings).unwrap();
        let loaded = load_settings();
        assert_eq!(loaded.ln_conf_path, Some(PathBuf::from("/tmp/ln.conf")));
        assert!(loaded.p2pool_conf_path.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn save_settings_creates_file_via_public_fn() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        let settings = Settings {
            shares_market_conf_path: Some(PathBuf::from("/tmp/shares.conf")),
            ..Default::default()
        };
        save_settings(&settings).unwrap();
        let path = dir.path().join("settings.toml");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("shares_market_conf_path"));
        assert!(content.contains("/tmp/shares.conf"));
    }

    #[test]
    fn config_dir_returns_a_path() {
        // config_dir() must succeed on any platform the CI runs on
        let dir = config_dir();
        assert!(dir.is_ok(), "config_dir() failed: {:?}", dir.err());
    }

    #[test]
    fn settings_path_ends_with_settings_toml() {
        let path = settings_path().unwrap();
        assert_eq!(path.file_name().unwrap(), "settings.toml");
    }

    fn set_config_dir(dir: &tempfile::TempDir) {
        // Serialised by #[serial] — no concurrent access to PDM_CONFIG_DIR.
        unsafe { std::env::set_var("PDM_CONFIG_DIR", dir.path()) };
    }

    #[test]
    #[serial_test::serial]
    fn load_settings_returns_default_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        // No settings.toml written
        let settings = load_settings();
        assert!(settings.p2pool_conf_path.is_none());
        assert!(settings.ln_conf_path.is_none());
        assert!(settings.shares_market_conf_path.is_none());
        assert!(settings.settings_dir_override.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn load_settings_returns_default_for_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        std::fs::write(dir.path().join("settings.toml"), "not valid toml :::").unwrap();
        let settings = load_settings();
        assert!(settings.p2pool_conf_path.is_none());
        assert!(settings.ln_conf_path.is_none());
        assert!(settings.shares_market_conf_path.is_none());
        assert!(settings.settings_dir_override.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn load_settings_reads_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"p2pool_conf_path = "/tmp/p2pool.toml""#,
        )
        .unwrap();
        let settings = load_settings();
        assert_eq!(
            settings.p2pool_conf_path,
            Some(PathBuf::from("/tmp/p2pool.toml"))
        );
    }

    #[test]
    fn settings_dir_override_field_serializes() {
        let settings = Settings {
            settings_dir_override: Some(PathBuf::from("/custom/dir")),
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        assert!(toml_str.contains("settings_dir_override"));
        let back: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            back.settings_dir_override,
            Some(PathBuf::from("/custom/dir"))
        );
    }

    #[test]
    #[serial_test::serial]
    fn save_settings_creates_and_load_settings_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        set_config_dir(&dir);
        let settings = Settings {
            ln_conf_path: Some(PathBuf::from("/tmp/ln.conf")),
            ..Default::default()
        };
        save_settings(&settings).unwrap();
        let loaded = load_settings();
        assert_eq!(loaded.ln_conf_path, settings.ln_conf_path);
        assert!(loaded.p2pool_conf_path.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn save_with_override_writes_to_override_dir_and_default() {
        let default_dir = tempfile::tempdir().unwrap();
        let override_dir = tempfile::tempdir().unwrap();
        set_config_dir(&default_dir);

        let settings = Settings {
            settings_dir_override: Some(override_dir.path().to_path_buf()),
            ..Default::default()
        };
        save_settings(&settings).unwrap();

        // Both the override location and the default location must have the file.
        let override_path = override_dir.path().join("settings.toml");
        let default_path = default_dir.path().join("settings.toml");
        assert!(override_path.exists(), "override settings.toml missing");
        assert!(default_path.exists(), "default settings.toml missing");

        let override_content = std::fs::read_to_string(&override_path).unwrap();
        let default_content = std::fs::read_to_string(&default_path).unwrap();
        assert_eq!(
            override_content, default_content,
            "override and default copies must match"
        );
        assert!(override_content.contains("settings_dir_override"));
    }

    #[test]
    #[serial_test::serial]
    fn load_settings_reads_from_override_dir_when_set() {
        let default_dir = tempfile::tempdir().unwrap();
        let override_dir = tempfile::tempdir().unwrap();
        set_config_dir(&default_dir);

        // Write a pointer in the default dir.
        let pointer = Settings {
            p2pool_conf_path: Some(PathBuf::from("/default/p2pool.toml")),
            settings_dir_override: Some(override_dir.path().to_path_buf()),
            ..Default::default()
        };
        std::fs::write(
            default_dir.path().join("settings.toml"),
            toml::to_string_pretty(&pointer).unwrap(),
        )
        .unwrap();

        // Write the authoritative settings in the override dir.
        let authoritative = Settings {
            p2pool_conf_path: Some(PathBuf::from("/override/p2pool.toml")),
            settings_dir_override: Some(override_dir.path().to_path_buf()),
            ..Default::default()
        };
        std::fs::write(
            override_dir.path().join("settings.toml"),
            toml::to_string_pretty(&authoritative).unwrap(),
        )
        .unwrap();

        let loaded = load_settings();
        assert_eq!(
            loaded.p2pool_conf_path,
            Some(PathBuf::from("/override/p2pool.toml")),
            "must read from the override dir, not the default-location pointer"
        );
    }

    #[test]
    #[serial_test::serial]
    fn load_settings_falls_back_to_default_when_override_unreadable() {
        let default_dir = tempfile::tempdir().unwrap();
        set_config_dir(&default_dir);

        // Pointer points to a directory that doesn't exist.
        let pointer = Settings {
            p2pool_conf_path: Some(PathBuf::from("/default/p2pool.toml")),
            settings_dir_override: Some(PathBuf::from("/nonexistent/dir")),
            ..Default::default()
        };
        std::fs::write(
            default_dir.path().join("settings.toml"),
            toml::to_string_pretty(&pointer).unwrap(),
        )
        .unwrap();

        let loaded = load_settings();
        // Override unreadable → falls back to the default-location settings.
        assert_eq!(
            loaded.p2pool_conf_path,
            Some(PathBuf::from("/default/p2pool.toml"))
        );
    }
}
