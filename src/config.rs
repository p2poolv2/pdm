// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use bitcoin::Network;
use p2poolv2_config::Config;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSection {
    Stratum,
    BitcoinRpc,
    Network,
    Store,
    Logging,
    Api,
}

impl fmt::Display for ConfigSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigSection::Stratum => write!(f, "stratum"),
            ConfigSection::BitcoinRpc => write!(f, "bitcoinrpc"),
            ConfigSection::Network => write!(f, "network"),
            ConfigSection::Store => write!(f, "store"),
            ConfigSection::Logging => write!(f, "logging"),
            ConfigSection::Api => write!(f, "api"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    Required,
    Optional { default: Option<String> },
}

#[derive(Debug, Clone)]
pub struct P2PoolFieldSchema {
    pub description: String,
    pub kind: FieldKind,
    pub type_hint: String,
    pub sensitive: bool,
}

/// A single editable TUI row — the view layer equivalent of
/// `P2PoolConfigEntry` in this module.
/// The external `p2poolv2_config` crate has no concept of this;
/// it only knows nested structs for deserialization.
#[derive(Debug, Clone)]
pub struct P2PoolConfigEntry {
    pub section: ConfigSection,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub schema: P2PoolFieldSchema,
}

impl P2PoolConfigEntry {
    fn required(
        section: ConfigSection,
        key: &str,
        value: String,
        description: &str,
        type_hint: &str,
    ) -> Self {
        Self {
            section,
            key: key.to_string(),
            value,
            enabled: true,
            schema: P2PoolFieldSchema {
                description: description.to_string(),
                kind: FieldKind::Required,
                type_hint: type_hint.to_string(),
                sensitive: false,
            },
        }
    }

    fn optional(
        section: ConfigSection,
        key: &str,
        value: Option<String>,
        description: &str,
        type_hint: &str,
        default: Option<&str>,
    ) -> Self {
        let enabled = value.is_some();
        Self {
            section,
            key: key.to_string(),
            value: value.unwrap_or_default(),
            enabled,
            schema: P2PoolFieldSchema {
                description: description.to_string(),
                kind: FieldKind::Optional {
                    default: default.map(str::to_string),
                },
                type_hint: type_hint.to_string(),
                sensitive: false,
            },
        }
    }

    fn sensitive(mut self) -> Self {
        self.schema.sensitive = true;
        self
    }
}

/// Flattens the nested `p2poolv2_config::Config` into a flat
/// `Vec<P2PoolConfigEntry>` that the TUI list can render row by row.
pub fn flatten_config(cfg: &Config) -> Vec<P2PoolConfigEntry> {
    let mut e: Vec<P2PoolConfigEntry> = Vec::new();
    let s = &cfg.stratum;

    // Stratum
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "hostname",
        s.hostname.clone(),
        "Stratum server hostname",
        "String",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "port",
        s.port.to_string(),
        "Stratum server port",
        "u16",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "start_difficulty",
        s.start_difficulty.to_string(),
        "Initial difficulty assigned to new miners",
        "u64",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "minimum_difficulty",
        s.minimum_difficulty.to_string(),
        "Minimum allowed difficulty",
        "u64",
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "maximum_difficulty",
        s.maximum_difficulty.map(|v| v.to_string()),
        "Maximum allowed difficulty (unset = unlimited)",
        "u64",
        None,
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "solo_address",
        s.solo_address.clone(),
        "Bitcoin address for solo mining payouts",
        "Address",
        None,
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "zmqpubhashblock",
        s.zmqpubhashblock.clone(),
        "ZMQ address for new block notifications",
        "URI",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "bootstrap_address",
        s.bootstrap_address.clone(),
        "Bitcoin address for first jobs before any share exists",
        "Address",
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "donation_address",
        s.donation_address.clone(),
        "Developer donation address (must pair with donation)",
        "Address",
        None,
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "donation",
        s.donation.map(|v| v.to_string()),
        "Developer donation in basis points (100 = 1%)",
        "u16",
        None,
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "fee_address",
        s.fee_address.clone(),
        "Pool fee address (must pair with fee)",
        "Address",
        None,
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "fee",
        s.fee.map(|v| v.to_string()),
        "Pool fee in basis points (100 = 1%)",
        "u16",
        None,
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "network",
        cfg.stratum.network.to_string(),
        "Bitcoin network: main / testnet4 / signet / regtest",
        "Network",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "version_mask",
        format!("{:x}", s.version_mask),
        "Version-rolling mask (hex)",
        "hex i32",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Stratum,
        "difficulty_multiplier",
        s.difficulty_multiplier.to_string(),
        "Multiplier for dynamic difficulty adjustment",
        "f64",
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "ignore_difficulty",
        s.ignore_difficulty.map(|v| v.to_string()),
        "Skip difficulty checks (test environments only)",
        "bool",
        Some("false"),
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Stratum,
        "pool_signature",
        s.pool_signature.clone(),
        "Coinbase pool signature (max 16 bytes)",
        "String",
        None,
    ));

    // BitcoinRPC
    let rpc = &cfg.bitcoinrpc;
    e.push(P2PoolConfigEntry::required(
        ConfigSection::BitcoinRpc,
        "url",
        rpc.url.clone(),
        "Bitcoin Core RPC endpoint URL",
        "URI",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::BitcoinRpc,
        "username",
        rpc.username.clone(),
        "Bitcoin RPC username",
        "String",
    ));
    e.push(
        P2PoolConfigEntry::required(
            ConfigSection::BitcoinRpc,
            "password",
            rpc.password.clone(),
            "Bitcoin RPC password",
            "String",
        )
        .sensitive(),
    );

    // Network
    let n = &cfg.network;
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "listen_address",
        n.listen_address.clone(),
        "P2P listen address (host:port)",
        "String",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "dial_peers",
        n.dial_peers.join(","),
        "Bootstrap peers, comma-separated (host:port,...)",
        "CSV",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "max_established_incoming",
        n.max_established_incoming.to_string(),
        "Maximum established inbound connections",
        "u32",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "max_established_outgoing",
        n.max_established_outgoing.to_string(),
        "Maximum established outbound connections",
        "u32",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "max_established_per_peer",
        n.max_established_per_peer.to_string(),
        "Maximum connections per individual peer",
        "u32",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "dial_timeout_secs",
        n.dial_timeout_secs.to_string(),
        "Timeout in seconds for outbound dial attempts",
        "u64",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Network,
        "max_requests_per_second",
        n.max_requests_per_second.to_string(),
        "Rate limit: max requests per second per peer",
        "u64",
    ));

    // Store
    let st = &cfg.store;
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Store,
        "path",
        st.path.clone(),
        "Path to the persistent share store",
        "Path",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Store,
        "background_task_frequency_hours",
        st.background_task_frequency_hours.to_string(),
        "How often to run background cleanup tasks (hours)",
        "u64",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Store,
        "pplns_ttl_days",
        st.pplns_ttl_days.to_string(),
        "Time-to-live for PPLNS shares (days)",
        "u64",
    ));

    // Logging
    let l = &cfg.logging;
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Logging,
        "file",
        l.file.clone(),
        "Log file path (omit to disable file logging)",
        "Path",
        None,
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Logging,
        "level",
        l.level.clone(),
        "Log verbosity: error / warn / info / debug / trace",
        "String",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Logging,
        "stats_dir",
        l.stats_dir.clone(),
        "Directory for stats output files",
        "Path",
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Logging,
        "console",
        l.console.map(|v| v.to_string()),
        "Log to stdout (true/false)",
        "bool",
        Some("true"),
    ));

    // API
    let a = &cfg.api;
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Api,
        "hostname",
        a.hostname.clone(),
        "API server hostname",
        "String",
    ));
    e.push(P2PoolConfigEntry::required(
        ConfigSection::Api,
        "port",
        a.port.to_string(),
        "API server port",
        "u16",
    ));
    e.push(P2PoolConfigEntry::optional(
        ConfigSection::Api,
        "auth_user",
        a.auth_user.clone(),
        "API authentication username",
        "String",
        None,
    ));
    e.push(
        P2PoolConfigEntry::optional(
            ConfigSection::Api,
            "auth_token",
            a.auth_token.clone(),
            "API auth token (salt$hmac, managed by server)",
            "String",
            None,
        )
        .sensitive(),
    );
    e.push(
        P2PoolConfigEntry::optional(
            ConfigSection::Api,
            "auth_password",
            a.auth_password.clone(),
            "API password for CLI client auth",
            "String",
            None,
        )
        .sensitive(),
    );

    e
}

/// Inner dispatch for config edits.
///
/// Matches a flattened `(section, key)` pair to the corresponding nested field inside `Config` and applies the parsed update.
///
/// This is split from `apply_edit()` so tests can directly inject
/// synthetic entries and explicitly verify the defensive `_ => unknown field` fallback branch, which `flatten_config()` cannot normally produce.
pub fn dispatch_edit(
    cfg: &mut Config,
    entry: &P2PoolConfigEntry,
    new_value: &str,
) -> Result<(), String> {
    match (&entry.section, entry.key.as_str()) {
        // Stratum
        (ConfigSection::Stratum, "hostname") => {
            cfg.stratum.hostname = new_value.to_string();
        }
        (ConfigSection::Stratum, "port") => {
            cfg.stratum.port = new_value.parse().map_err(|_| "port must be 0–65535")?;
        }
        (ConfigSection::Stratum, "start_difficulty") => {
            cfg.stratum.start_difficulty = new_value.parse().map_err(|_| "must be u64")?;
        }
        (ConfigSection::Stratum, "minimum_difficulty") => {
            cfg.stratum.minimum_difficulty = new_value.parse().map_err(|_| "must be u64")?;
        }
        (ConfigSection::Stratum, "maximum_difficulty") => {
            cfg.stratum.maximum_difficulty = if new_value.is_empty() {
                None
            } else {
                Some(new_value.parse().map_err(|_| "must be u64")?)
            };
        }
        (ConfigSection::Stratum, "solo_address") => {
            cfg.stratum.solo_address = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Stratum, "zmqpubhashblock") => {
            cfg.stratum.zmqpubhashblock = new_value.to_string();
        }
        (ConfigSection::Stratum, "bootstrap_address") => {
            cfg.stratum.bootstrap_address = new_value.to_string();
        }
        (ConfigSection::Stratum, "donation_address") => {
            cfg.stratum.donation_address = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Stratum, "donation") => {
            cfg.stratum.donation = if new_value.is_empty() {
                None
            } else {
                Some(new_value.parse().map_err(|_| "must be u16")?)
            };
        }
        (ConfigSection::Stratum, "fee_address") => {
            cfg.stratum.fee_address = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Stratum, "fee") => {
            cfg.stratum.fee = if new_value.is_empty() {
                None
            } else {
                Some(new_value.parse().map_err(|_| "must be u16")?)
            };
        }
        (ConfigSection::Stratum, "network") => {
            cfg.stratum.network = Network::from_core_arg(new_value)
                .map_err(|_| "must be main / testnet4 / signet / regtest")?;
        }
        (ConfigSection::Stratum, "version_mask") => {
            cfg.stratum.version_mask = i32::from_str_radix(new_value.trim_start_matches("0x"), 16)
                .map_err(|_| "must be hex (e.g. 1fffe000)")?;
        }
        (ConfigSection::Stratum, "difficulty_multiplier") => {
            cfg.stratum.difficulty_multiplier = new_value.parse().map_err(|_| "must be f64")?;
        }
        (ConfigSection::Stratum, "ignore_difficulty") => {
            cfg.stratum.ignore_difficulty = if new_value.is_empty() {
                None
            } else {
                Some(new_value.parse().map_err(|_| "must be true or false")?)
            };
        }
        (ConfigSection::Stratum, "pool_signature") => {
            cfg.stratum.pool_signature = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }

        // BitcoinRPC
        (ConfigSection::BitcoinRpc, "url") => {
            cfg.bitcoinrpc.url = new_value.to_string();
        }
        (ConfigSection::BitcoinRpc, "username") => {
            cfg.bitcoinrpc.username = new_value.to_string();
        }
        (ConfigSection::BitcoinRpc, "password") => {
            cfg.bitcoinrpc.password = new_value.to_string();
        }

        // Network
        (ConfigSection::Network, "listen_address") => {
            cfg.network.listen_address = new_value.to_string();
        }
        (ConfigSection::Network, "dial_peers") => {
            cfg.network.dial_peers = if new_value.is_empty() {
                vec![]
            } else {
                new_value.split(',').map(|s| s.trim().to_string()).collect()
            };
        }
        (ConfigSection::Network, "max_established_incoming") => {
            cfg.network.max_established_incoming = new_value.parse().map_err(|_| "must be u32")?;
        }
        (ConfigSection::Network, "max_established_outgoing") => {
            cfg.network.max_established_outgoing = new_value.parse().map_err(|_| "must be u32")?;
        }
        (ConfigSection::Network, "max_established_per_peer") => {
            cfg.network.max_established_per_peer = new_value.parse().map_err(|_| "must be u32")?;
        }
        (ConfigSection::Network, "dial_timeout_secs") => {
            cfg.network.dial_timeout_secs = new_value.parse().map_err(|_| "must be u64")?;
        }
        (ConfigSection::Network, "max_requests_per_second") => {
            cfg.network.max_requests_per_second = new_value.parse().map_err(|_| "must be u64")?;
        }

        // Store
        (ConfigSection::Store, "path") => {
            cfg.store.path = new_value.to_string();
        }
        (ConfigSection::Store, "background_task_frequency_hours") => {
            cfg.store.background_task_frequency_hours =
                new_value.parse().map_err(|_| "must be u64")?;
        }
        (ConfigSection::Store, "pplns_ttl_days") => {
            cfg.store.pplns_ttl_days = new_value.parse().map_err(|_| "must be u64")?;
        }

        // Logging
        (ConfigSection::Logging, "file") => {
            cfg.logging.file = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Logging, "level") => {
            cfg.logging.level = new_value.to_string();
        }
        (ConfigSection::Logging, "stats_dir") => {
            cfg.logging.stats_dir = new_value.to_string();
        }
        (ConfigSection::Logging, "console") => {
            cfg.logging.console = if new_value.is_empty() {
                None
            } else {
                Some(new_value.parse().map_err(|_| "must be true or false")?)
            };
        }

        // API
        (ConfigSection::Api, "hostname") => {
            cfg.api.hostname = new_value.to_string();
        }
        (ConfigSection::Api, "port") => {
            cfg.api.port = new_value.parse().map_err(|_| "port must be 0–65535")?;
        }
        (ConfigSection::Api, "auth_user") => {
            cfg.api.auth_user = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Api, "auth_token") => {
            cfg.api.auth_token = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }
        (ConfigSection::Api, "auth_password") => {
            cfg.api.auth_password = if new_value.is_empty() {
                None
            } else {
                Some(new_value.to_string())
            };
        }

        _ => return Err(format!("unknown field: {}.{}", entry.section, entry.key)),
    }
    Ok(())
}

/// Writes an edited flat-row value back into `Config`.
/// Resolves the selected row from `flatten_config()` and delegates the actual update to `dispatch_edit()`.
/// Returns `Err` if the index is invalid or parsing fails
pub fn apply_edit(cfg: &mut Config, index: usize, new_value: &str) -> Result<(), String> {
    let entries = flatten_config(cfg);
    let entry = entries
        .get(index)
        .ok_or_else(|| "index out of range".to_string())?;
    dispatch_edit(cfg, entry, new_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2poolv2_config::Config;
    use tempfile::tempdir;

    fn make_config() -> Config {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[stratum]
hostname = "127.0.0.1"
port = 3333
start_difficulty = 1000
minimum_difficulty = 100
maximum_difficulty = 100000
zmqpubhashblock = "tcp://127.0.0.1:28332"
bootstrap_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
network = "signet"
version_mask = "1fffe000"
difficulty_multiplier = 1.0
pool_signature = "MyPool/1.0"

[bitcoinrpc]
url = "http://127.0.0.1:38332"
username = "rpcuser"
password = "rpcpassword"

[network]
listen_address = "0.0.0.0:8333"
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
max_requests_per_second = 1
dial_timeout_secs = 30

[store]
path = "./data/store"
background_task_frequency_hours = 1
pplns_ttl_days = 7

[logging]
level = "info"
stats_dir = "./logs/stats"
console = true

[api]
hostname = "127.0.0.1"
port = 3030
        "#,
        )
        .unwrap();

        // keep dir alive until Config is loaded
        Config::load(path.to_str().unwrap()).expect("inline test config must parse")
    }

    #[test]
    fn flatten_produces_entries_for_all_sections() {
        let cfg = make_config();
        let entries = flatten_config(&cfg);
        assert!(entries.iter().any(|e| e.section == ConfigSection::Stratum));
        assert!(
            entries
                .iter()
                .any(|e| e.section == ConfigSection::BitcoinRpc)
        );
        assert!(entries.iter().any(|e| e.section == ConfigSection::Network));
        assert!(entries.iter().any(|e| e.section == ConfigSection::Store));
        assert!(entries.iter().any(|e| e.section == ConfigSection::Logging));
        assert!(entries.iter().any(|e| e.section == ConfigSection::Api));
    }

    #[test]
    fn sensitive_fields_are_marked() {
        let cfg = make_config();
        let entries = flatten_config(&cfg);
        let password = entries
            .iter()
            .find(|e| e.section == ConfigSection::BitcoinRpc && e.key == "password")
            .expect("password entry must exist");
        assert!(password.schema.sensitive);
    }

    #[test]
    fn apply_edit_stratum_port_roundtrip() {
        let mut cfg = make_config();
        let idx = flatten_config(&cfg)
            .iter()
            .position(|e| e.section == ConfigSection::Stratum && e.key == "port")
            .unwrap();
        apply_edit(&mut cfg, idx, "4444").unwrap();
        assert_eq!(cfg.stratum.port, 4444);
    }

    #[test]
    fn apply_edit_rejects_bad_port() {
        let mut cfg = make_config();
        let idx = flatten_config(&cfg)
            .iter()
            .position(|e| e.section == ConfigSection::Stratum && e.key == "port")
            .unwrap();
        assert!(apply_edit(&mut cfg, idx, "notanumber").is_err());
    }

    #[test]
    fn apply_edit_optional_empty_clears_field() {
        let mut cfg = make_config();
        cfg.stratum.pool_signature = Some("MyPool".to_string());
        let idx = flatten_config(&cfg)
            .iter()
            .position(|e| e.section == ConfigSection::Stratum && e.key == "pool_signature")
            .unwrap();
        apply_edit(&mut cfg, idx, "").unwrap();
        assert!(cfg.stratum.pool_signature.is_none());
    }

    #[test]
    fn apply_edit_dial_peers_csv_roundtrip() {
        let mut cfg = make_config();
        let idx = flatten_config(&cfg)
            .iter()
            .position(|e| e.section == ConfigSection::Network && e.key == "dial_peers")
            .unwrap();
        apply_edit(&mut cfg, idx, "a:1,b:2").unwrap();
        assert_eq!(cfg.network.dial_peers, vec!["a:1", "b:2"]);
    }

    #[test]
    fn apply_edit_out_of_range_returns_err() {
        let mut cfg = make_config();
        assert!(apply_edit(&mut cfg, 9999, "x").is_err());
    }

    #[test]
    fn config_section_display_outputs_correct_strings() {
        use super::ConfigSection;

        let cases = vec![
            (ConfigSection::Stratum, "stratum"),
            (ConfigSection::BitcoinRpc, "bitcoinrpc"),
            (ConfigSection::Network, "network"),
            (ConfigSection::Store, "store"),
            (ConfigSection::Logging, "logging"),
            (ConfigSection::Api, "api"),
        ];

        for (section, expected) in cases {
            assert_eq!(section.to_string(), expected);
        }
    }

    #[test]
    fn apply_edit_covers_multiple_field_types() {
        use super::{ConfigSection, apply_edit, flatten_config};

        let mut cfg = make_config();
        let entries = flatten_config(&cfg);

        let idx = |section: ConfigSection, key: &str| {
            entries
                .iter()
                .position(|e| e.section == section && e.key == key)
                .expect("field must exist")
        };

        // String field
        apply_edit(
            &mut cfg,
            idx(ConfigSection::Stratum, "hostname"),
            "new.host",
        )
        .unwrap();
        assert_eq!(cfg.stratum.hostname, "new.host");

        // Numeric field
        apply_edit(&mut cfg, idx(ConfigSection::Stratum, "port"), "5555").unwrap();
        assert_eq!(cfg.stratum.port, 5555);

        // Optional cleared
        apply_edit(&mut cfg, idx(ConfigSection::Stratum, "pool_signature"), "").unwrap();
        assert!(cfg.stratum.pool_signature.is_none());

        // Optional set
        apply_edit(&mut cfg, idx(ConfigSection::Stratum, "donation"), "25").unwrap();
        assert_eq!(cfg.stratum.donation, Some(25));

        // CSV parsing
        apply_edit(
            &mut cfg,
            idx(ConfigSection::Network, "dial_peers"),
            "a:1,b:2",
        )
        .unwrap();
        assert_eq!(cfg.network.dial_peers, vec!["a:1", "b:2"]);

        // Enum parsing
        apply_edit(&mut cfg, idx(ConfigSection::Stratum, "network"), "signet").unwrap();
        assert_eq!(cfg.stratum.network.to_string(), "signet");

        // Hex parsing
        apply_edit(
            &mut cfg,
            idx(ConfigSection::Stratum, "version_mask"),
            "1fffe000",
        )
        .unwrap();
        assert_eq!(cfg.stratum.version_mask, 0x1fffe000);

        // Bool parsing
        apply_edit(&mut cfg, idx(ConfigSection::Logging, "console"), "false").unwrap();
        assert_eq!(cfg.logging.console, Some(false));
    }

    #[test]
    fn apply_edit_rejects_invalid_inputs() {
        use super::{apply_edit, flatten_config};

        let mut cfg = make_config();
        let entries = flatten_config(&cfg);

        // Invalid number
        let port_idx = entries.iter().position(|e| e.key == "port").unwrap();
        assert!(apply_edit(&mut cfg, port_idx, "notanumber").is_err());

        // Invalid bool
        let console_idx = entries.iter().position(|e| e.key == "console").unwrap();
        assert!(apply_edit(&mut cfg, console_idx, "notabool").is_err());

        // Invalid hex
        let hex_idx = entries
            .iter()
            .position(|e| e.key == "version_mask")
            .unwrap();
        assert!(apply_edit(&mut cfg, hex_idx, "zzzz").is_err());
    }

    #[test]
    fn apply_edit_unknown_field_hits_fallback_branch() {
        let mut cfg = make_config();

        // Construct a fake entry with a key that exists in no match arm.
        // In normal use flatten_config never produces unknown keys —
        // this branch is a defensive guard against future bugs (e.g. a
        // new field added to flatten_config but forgotten in dispatch_edit).
        let fake_entry = P2PoolConfigEntry {
            section: ConfigSection::Api,
            key: "nonexistent_key_xyz".to_string(),
            value: String::new(),
            enabled: true,
            schema: P2PoolFieldSchema {
                description: "fake".to_string(),
                kind: FieldKind::Required,
                type_hint: "String".to_string(),
                sensitive: false,
            },
        };
        let result = dispatch_edit(&mut cfg, &fake_entry, "value");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("unknown field"),
            "error message must mention unknown field"
        );
    }

    #[test]
    fn apply_edit_out_of_bounds_index_returns_error() {
        let mut cfg = make_config();
        // usize::MAX is always out of range — hits the bounds check
        // before any section/key dispatch occurs.
        let result = apply_edit(&mut cfg, usize::MAX, "value");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("index out of range"),
            "error message must mention index out of range"
        );
    }
}
