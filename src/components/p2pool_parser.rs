// SPDX-FileCopyrightText: 2024 PDM Authors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigField {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSection {
    pub title: String,
    pub fields: Vec<ConfigField>,
}

/// Parses a p2pool config file into organized sections.
pub fn parse_p2pool_config<P: AsRef<Path>>(path: P) -> io::Result<Vec<ConfigSection>> {
    let file = File::open(path)?;
    let reader = io::BufReader::new(file);

    // Use a temporary map to bucket fields
    let mut sections_map: std::collections::HashMap<String, Vec<ConfigField>> =
        std::collections::HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse key=value
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // P2POOL SPECIFIC CATEGORIZATION
            // This logic is unique to P2Pool's expected keys
            let section = if key.contains("user") || key.contains("pass") || key.contains("auth") {
                "Authentication"
            } else if key.starts_with("bitcoind") {
                "Bitcoin Node"
            } else if key.contains("port") || key.contains("address") || key.contains("listen") {
                "Network"
            } else if key.contains("payout") || key.contains("wallet") {
                "Payouts"
            } else {
                "General Settings"
            };

            sections_map
                .entry(section.to_string())
                .or_default()
                .push(ConfigField { key, value });
        }
    }

    // Sort sections alphabetically for consistent UI
    let mut sections: Vec<ConfigSection> = sections_map
        .into_iter()
        .map(|(title, fields)| ConfigSection { title, fields })
        .collect();

    sections.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(sections)
}
