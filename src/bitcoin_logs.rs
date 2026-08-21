// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::bitcoin_config::ConfigEntry;
use crate::settings::Settings;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_LOG_LINES: usize = 300;
pub const MAX_LOG_READ_BYTES: u64 = 512 * 1024;

const READ_CHUNK_SIZE: u64 = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitcoinLogSnapshot {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitcoinLogNetwork {
    Mainnet,
    Testnet,
    Testnet4,
    Signet,
    Regtest,
}

pub fn read_log_snapshot(path: &Path, max_lines: usize) -> Result<BitcoinLogSnapshot> {
    let lines = read_recent_log_lines(path, max_lines)?;
    let file_size = std::fs::metadata(path)
        .with_context(|| format!("could not read metadata for {}", path.display()))?
        .len();

    Ok(BitcoinLogSnapshot {
        path: path.to_path_buf(),
        lines,
        file_size,
    })
}

pub fn read_recent_log_lines(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    if max_lines == 0 {
        return Ok(Vec::new());
    }

    let mut file =
        File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let file_len = file
        .metadata()
        .with_context(|| format!("could not read metadata for {}", path.display()))?
        .len();

    if file_len == 0 {
        return Ok(Vec::new());
    }

    let window_start = file_len.saturating_sub(MAX_LOG_READ_BYTES);
    let mut position = file_len;
    let mut newline_count = 0usize;
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut partial_prefix = false;

    if file_len > MAX_LOG_READ_BYTES {
        let mut prev_byte = [0u8; 1];
        file.seek(SeekFrom::Start(window_start.saturating_sub(1)))
            .with_context(|| format!("could not seek {}", path.display()))?;
        file.read_exact(&mut prev_byte)
            .with_context(|| format!("could not read {}", path.display()))?;
        partial_prefix = prev_byte[0] != b'\n';
    }

    while position > window_start && newline_count < max_lines {
        let remaining_bytes = position.saturating_sub(window_start);
        let read_size = READ_CHUNK_SIZE.min(remaining_bytes);
        let chunk_start = position - read_size;

        file.seek(SeekFrom::Start(chunk_start))
            .with_context(|| format!("could not seek {}", path.display()))?;

        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)
            .with_context(|| format!("could not read {}", path.display()))?;

        newline_count += chunk.iter().filter(|byte| **byte == b'\n').count();
        chunks.push(chunk);
        position = chunk_start;
    }

    let total_len = chunks.iter().map(Vec::len).sum();
    let mut bytes = Vec::with_capacity(total_len);
    for chunk in chunks.into_iter().rev() {
        bytes.extend(chunk);
    }

    let mut lines: Vec<String> = String::from_utf8_lossy(&bytes)
        .lines()
        .map(ToOwned::to_owned)
        .collect();

    if partial_prefix && !lines.is_empty() {
        lines.remove(0);
    }

    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }

    lines.reverse();
    Ok(lines)
}

pub fn resolve_log_path(settings: &Settings, entries: &[ConfigEntry]) -> Option<PathBuf> {
    if let Some(path) = &settings.bitcoin_core_log_path {
        return Some(expand_path(path));
    }

    if let Some(data_dir) = &settings.bitcoin_core_data_dir {
        return Some(best_log_path_for_data_dir(&expand_path(data_dir), entries));
    }

    if let Some(data_dir) = configured_data_dir(entries) {
        return Some(best_log_path_for_data_dir(&data_dir, entries));
    }

    default_data_dirs()
        .into_iter()
        .find_map(|data_dir| existing_log_path_for_data_dir(&data_dir, entries))
}

pub fn best_log_path_for_data_dir(data_dir: &Path, entries: &[ConfigEntry]) -> PathBuf {
    existing_log_path_for_data_dir(data_dir, entries).unwrap_or_else(|| {
        log_path_candidates_for_data_dir(data_dir, entries)
            .into_iter()
            .next()
            .unwrap_or_else(|| data_dir.join("debug.log"))
    })
}

pub fn log_path_candidates_for_data_dir(data_dir: &Path, entries: &[ConfigEntry]) -> Vec<PathBuf> {
    let data_dir = expand_path(data_dir);
    let network = network_from_entries(entries);
    let mut suffixes = Vec::new();

    match network {
        BitcoinLogNetwork::Mainnet => {
            suffixes.push(PathBuf::from("debug.log"));
        }
        BitcoinLogNetwork::Testnet => {
            suffixes.push(PathBuf::from("testnet3/debug.log"));
            suffixes.push(PathBuf::from("testnet/debug.log"));
        }
        BitcoinLogNetwork::Testnet4 => {
            suffixes.push(PathBuf::from("testnet4/debug.log"));
        }
        BitcoinLogNetwork::Signet => {
            suffixes.push(PathBuf::from("signet/debug.log"));
        }
        BitcoinLogNetwork::Regtest => {
            suffixes.push(PathBuf::from("regtest/debug.log"));
        }
    }

    // Also support selecting the network-specific directory itself and common
    // layouts that differ from the currently selected chain.
    suffixes.extend([
        PathBuf::from("debug.log"),
        PathBuf::from("testnet3/debug.log"),
        PathBuf::from("testnet4/debug.log"),
        PathBuf::from("testnet/debug.log"),
        PathBuf::from("signet/debug.log"),
        PathBuf::from("regtest/debug.log"),
    ]);

    let mut candidates = Vec::new();
    for suffix in suffixes {
        let candidate = data_dir.join(suffix);
        if !candidates.iter().any(|path| path == &candidate) {
            candidates.push(candidate);
        }
    }

    candidates
}

pub fn expand_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    expand_path_str(raw.trim())
}

pub fn expand_path_str(raw: &str) -> PathBuf {
    let mut expanded = raw.to_string();

    if expanded == "~" || expanded.starts_with("~/") {
        if let Some(home) = home_dir() {
            let suffix = expanded.trim_start_matches('~').trim_start_matches('/');
            expanded = home.join(suffix).to_string_lossy().into_owned();
        }
    }

    if expanded.contains("%APPDATA%")
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        expanded = expanded.replace("%APPDATA%", &appdata.to_string_lossy());
    }

    PathBuf::from(expanded)
}

fn existing_log_path_for_data_dir(data_dir: &Path, entries: &[ConfigEntry]) -> Option<PathBuf> {
    log_path_candidates_for_data_dir(data_dir, entries)
        .into_iter()
        .find(|path| path.is_file())
}

fn configured_data_dir(entries: &[ConfigEntry]) -> Option<PathBuf> {
    entry_value(entries, "datadir")
        .map(expand_path_str)
        .filter(|path| !path.as_os_str().is_empty())
}

fn default_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = home_dir() {
        dirs.push(home.join(".bitcoin"));
        dirs.push(home.join("Library/Application Support/Bitcoin"));
    }

    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Bitcoin"));
    } else if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        dirs.push(
            PathBuf::from(user_profile)
                .join("AppData")
                .join("Roaming")
                .join("Bitcoin"),
        );
    }

    dirs
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn entry_value<'a>(entries: &'a [ConfigEntry], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|entry| entry.enabled && entry.key == key && !entry.value.trim().is_empty())
        .map(|entry| entry.value.trim())
}

fn network_from_entries(entries: &[ConfigEntry]) -> BitcoinLogNetwork {
    if bool_entry(entries, "regtest") {
        return BitcoinLogNetwork::Regtest;
    }
    if bool_entry(entries, "signet") {
        return BitcoinLogNetwork::Signet;
    }
    if bool_entry(entries, "testnet4") {
        return BitcoinLogNetwork::Testnet4;
    }
    if bool_entry(entries, "testnet") {
        return BitcoinLogNetwork::Testnet;
    }

    match entry_value(entries, "chain")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "test" | "testnet" | "testnet3" => BitcoinLogNetwork::Testnet,
        "testnet4" => BitcoinLogNetwork::Testnet4,
        "signet" => BitcoinLogNetwork::Signet,
        "regtest" => BitcoinLogNetwork::Regtest,
        _ => BitcoinLogNetwork::Mainnet,
    }
}

fn bool_entry(entries: &[ConfigEntry], key: &str) -> bool {
    matches!(
        entry_value(entries, key)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(key: &str, value: &str) -> ConfigEntry {
        ConfigEntry {
            key: key.to_string(),
            value: value.to_string(),
            schema: None,
            enabled: true,
            section: None,
        }
    }

    #[test]
    fn reads_recent_lines_newest_first() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(
            &path,
            "2026-01-01T00:00:00Z first\n2026-01-01T00:00:01Z second\n2026-01-01T00:00:02Z third\n",
        )
        .unwrap();

        let lines = read_recent_log_lines(&path, 2).unwrap();

        assert_eq!(
            lines,
            vec![
                "2026-01-01T00:00:02Z third".to_string(),
                "2026-01-01T00:00:01Z second".to_string(),
            ]
        );
    }

    #[test]
    fn reads_all_available_lines_when_fewer_than_requested() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();

        let lines = read_recent_log_lines(&path, 5).unwrap();

        assert_eq!(
            lines,
            vec!["three".to_string(), "two".to_string(), "one".to_string(),]
        );
    }

    #[test]
    fn reads_exact_boundary_line_counts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, "a\nb\nc\nd\n").unwrap();

        let lines = read_recent_log_lines(&path, 2).unwrap();

        assert_eq!(lines, vec!["d".to_string(), "c".to_string()]);
    }

    #[test]
    fn reads_tail_from_end_for_large_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("debug.log");
        let long_prefix = "x".repeat(MAX_LOG_READ_BYTES as usize + 1024);
        std::fs::write(&path, format!("{long_prefix}\nalpha\nbeta\ngamma\n")).unwrap();

        let lines = read_recent_log_lines(&path, 3).unwrap();

        assert_eq!(
            lines,
            vec!["gamma".to_string(), "beta".to_string(), "alpha".to_string()]
        );
    }

    #[test]
    fn returns_empty_for_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, "").unwrap();

        assert!(read_recent_log_lines(&path, 5).unwrap().is_empty());
    }

    #[test]
    fn resolves_direct_log_path_from_settings() {
        let settings = Settings {
            bitcoin_core_log_path: Some(PathBuf::from("/tmp/bitcoin/debug.log")),
            ..Default::default()
        };

        assert_eq!(
            resolve_log_path(&settings, &[]),
            Some(PathBuf::from("/tmp/bitcoin/debug.log"))
        );
    }

    #[test]
    fn prefers_network_layout_for_configured_data_dir() {
        let entries = vec![entry("signet", "1")];
        let path = best_log_path_for_data_dir(Path::new("/tmp/bitcoin"), &entries);

        assert_eq!(path, PathBuf::from("/tmp/bitcoin/signet/debug.log"));
    }

    #[test]
    fn finds_existing_testnet4_log() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("testnet4");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("debug.log"), "hello\n").unwrap();

        let entries = vec![entry("chain", "testnet4")];
        let path = existing_log_path_for_data_dir(dir.path(), &entries).unwrap();

        assert_eq!(path, log_dir.join("debug.log"));
    }
}
