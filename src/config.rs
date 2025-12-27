// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use config::{Config, File, FileFormat};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigType {
    String,
    Integer,
    Boolean,
}

#[derive(Debug, Clone)]
pub struct ConfigSchema {
    pub key: String,
    pub value_type: ConfigType,
    pub section: String,
    pub description: String,
    pub default: String,
}

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub schema: Option<ConfigSchema>,
    pub enabled: bool,
}

fn get_default_schema() -> Vec<ConfigSchema> {
    vec![
        // Core
        ConfigSchema {
            key: "datadir".to_string(),
            value_type: ConfigType::String,
            section: "Core".to_string(),
            description: "Directory to store data.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "txindex".to_string(),
            value_type: ConfigType::Boolean,
            section: "Core".to_string(),
            description: "Maintain a full transaction index.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "prune".to_string(),
            value_type: ConfigType::Integer,
            section: "Core".to_string(),
            description: "Reduce storage requirements by enabling pruning (deleting) of old blocks. 0 = disable.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "blocksonly".to_string(),
            value_type: ConfigType::Boolean,
            section: "Core".to_string(),
            description: "Reject transactions from network peers.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "dbcache".to_string(),
            value_type: ConfigType::Integer,
            section: "Core".to_string(),
            description: "Database cache size in megabytes.".to_string(),
            default: "450".to_string(),
        },
        ConfigSchema {
            key: "maxmempool".to_string(),
            value_type: ConfigType::Integer,
            section: "Core".to_string(),
            description: "Keep the transaction memory pool below <n> megabytes.".to_string(),
            default: "300".to_string(),
        },
         ConfigSchema {
            key: "pid".to_string(),
            value_type: ConfigType::String,
            section: "Core".to_string(),
            description: "Specify pid file. Relative paths will be prefixed by a net-specific datadir location.".to_string(),
            default: "bitcoind.pid".to_string(),
        },

        // Network
        ConfigSchema {
            key: "testnet".to_string(),
            value_type: ConfigType::Boolean,
            section: "Network".to_string(),
            description: "Run on the test network.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "regtest".to_string(),
            value_type: ConfigType::Boolean,
            section: "Network".to_string(),
            description: "Run on the regression test network.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "signet".to_string(),
            value_type: ConfigType::Boolean,
            section: "Network".to_string(),
            description: "Run on the signet network.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "listen".to_string(),
            value_type: ConfigType::Boolean,
            section: "Network".to_string(),
            description: "Accept connections from outside.".to_string(),
            default: "1".to_string(),
        },
        ConfigSchema {
            key: "bind".to_string(),
            value_type: ConfigType::String,
            section: "Network".to_string(),
            description: "Bind to given address and always listen on it. Use [host]:port notation for IPv6.".to_string(),
            default: "0.0.0.0".to_string(),
        },
        ConfigSchema {
            key: "port".to_string(),
            value_type: ConfigType::Integer,
            section: "Network".to_string(),
            description: "Listen for connections on <port>.".to_string(),
            default: "8333".to_string(),
        },
        ConfigSchema {
            key: "maxconnections".to_string(),
            value_type: ConfigType::Integer,
            section: "Network".to_string(),
            description: "Maintain at most <n> connections to peers.".to_string(),
            default: "125".to_string(),
        },
        ConfigSchema {
            key: "proxy".to_string(),
            value_type: ConfigType::String,
            section: "Network".to_string(),
            description: "Connect through SOCKS5 proxy.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "onion".to_string(),
            value_type: ConfigType::String,
            section: "Network".to_string(),
            description: "Use separate SOCKS5 proxy to reach peers via Tor onion services.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "upnp".to_string(),
            value_type: ConfigType::Boolean,
            section: "Network".to_string(),
            description: "Use UPnP to map the listening port.".to_string(),
            default: "0".to_string(),
        },

        // RPC
        ConfigSchema {
            key: "server".to_string(),
            value_type: ConfigType::Boolean,
            section: "RPC".to_string(),
            description: "Accept command line and JSON-RPC commands.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "rpcuser".to_string(),
            value_type: ConfigType::String,
            section: "RPC".to_string(),
            description: "Username for JSON-RPC connections.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "rpcpassword".to_string(),
            value_type: ConfigType::String,
            section: "RPC".to_string(),
            description: "Password for JSON-RPC connections.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "rpcauth".to_string(),
            value_type: ConfigType::String,
            section: "RPC".to_string(),
            description: "Username and hashed password for JSON-RPC connections.".to_string(),
            default: "".to_string(),
        },
         ConfigSchema {
            key: "rpcport".to_string(),
            value_type: ConfigType::Integer,
            section: "RPC".to_string(),
            description: "Listen for JSON-RPC connections on <port>.".to_string(),
            default: "8332".to_string(),
        },
        ConfigSchema {
            key: "rpcbind".to_string(),
            value_type: ConfigType::String,
            section: "RPC".to_string(),
            description: "Bind to given address to listen for JSON-RPC connections.".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "rpcallowip".to_string(),
            value_type: ConfigType::String,
            section: "RPC".to_string(),
            description: "Allow JSON-RPC connections from specified source. Valid for <ip> are a single IP (e.g. 1.2.3.4), a network/netmask (e.g. 1.2.3.4/255.255.255.0) or a network/CIDR (e.g. 1.2.3.4/24).".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "rpcthreads".to_string(),
            value_type: ConfigType::Integer,
            section: "RPC".to_string(),
            description: "Set the number of threads to service RPC calls.".to_string(),
            default: "4".to_string(),
        },

        // Wallet
        ConfigSchema {
            key: "disablewallet".to_string(),
            value_type: ConfigType::Boolean,
            section: "Wallet".to_string(),
            description: "Do not load the wallet and disable wallet RPC calls.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "fallbackfee".to_string(),
            value_type: ConfigType::String,
            section: "Wallet".to_string(),
            description: "A fee rate (in BTC/kvB) that will be used when fee estimation has insufficient data.".to_string(),
            default: "0.00021".to_string(),
        },
        ConfigSchema {
            key: "discardfee".to_string(),
            value_type: ConfigType::String,
            section: "Wallet".to_string(),
            description: "The fee rate (in BTC/kvB) that indicates your tolerance for discarding change by adding it to the fee.".to_string(),
            default: "0.0001".to_string(),
        },
        ConfigSchema {
            key: "mintxfee".to_string(),
            value_type: ConfigType::String,
            section: "Wallet".to_string(),
            description: "Fees (in BTC/kvB) smaller than this are considered zero fee for transaction creation.".to_string(),
            default: "0.00001".to_string(),
        },
        ConfigSchema {
            key: "paytxfee".to_string(),
            value_type: ConfigType::String,
            section: "Wallet".to_string(),
            description: "Fee (in BTC/kvB) to add to transactions you send.".to_string(),
            default: "0.00".to_string(),
        },

        // Debug
        ConfigSchema {
            key: "debug".to_string(),
            value_type: ConfigType::String,
            section: "Debug".to_string(),
            description: "Output debugging information (default: 0, supplying <category> is optional).".to_string(),
            default: "".to_string(),
        },
        ConfigSchema {
            key: "logips".to_string(),
            value_type: ConfigType::Boolean,
            section: "Debug".to_string(),
            description: "Include IP addresses in debug output.".to_string(),
            default: "0".to_string(),
        },
        ConfigSchema {
            key: "shrinkdebugfile".to_string(),
            value_type: ConfigType::Boolean,
            section: "Debug".to_string(),
            description: "Shrink debug.log file on client startup (default: 1 when no -debug).".to_string(),
            default: "1".to_string(),
        },

        // Mining
         ConfigSchema {
            key: "blockmaxweight".to_string(),
            value_type: ConfigType::Integer,
            section: "Mining".to_string(),
            description: "Set maximum BIP141 block weight (default: 3996000).".to_string(),
            default: "3996000".to_string(),
        },
        ConfigSchema {
            key: "minrelaytxfee".to_string(),
            value_type: ConfigType::String,
            section: "Mining".to_string(),
            description: "Fees (in BTC/kvB) smaller than this are considered zero fee for relaying, mining and transaction creation.".to_string(),
            default: "0.00001".to_string(),
        },

        // ZMQ
        ConfigSchema {
            key: "zmqpubhashblock".to_string(),
            value_type: ConfigType::String,
            section: "ZMQ".to_string(),
            description: "Enable publish hash block in <address>.".to_string(),
            default: "tcp://127.0.0.1:28332".to_string(),
        },
        ConfigSchema {
            key: "zmqpubhashtx".to_string(),
            value_type: ConfigType::String,
            section: "ZMQ".to_string(),
            description: "Enable publish hash transaction in <address>.".to_string(),
            default: "tcp://127.0.0.1:28332".to_string(),
        },
        ConfigSchema {
            key: "zmqpubrawblock".to_string(),
            value_type: ConfigType::String,
            section: "ZMQ".to_string(),
            description: "Enable publish raw block in <address>.".to_string(),
            default: "tcp://127.0.0.1:28332".to_string(),
        },
        ConfigSchema {
            key: "zmqpubrawtx".to_string(),
            value_type: ConfigType::String,
            section: "ZMQ".to_string(),
            description: "Enable publish raw transaction in <address>.".to_string(),
            default: "tcp://127.0.0.1:28332".to_string(),
        },
    ]
}

/// Parse bitcoin.conf file
pub fn parse_config(path: &Path) -> Result<Vec<ConfigEntry>> {
    let schema_list = get_default_schema();
    let mut entries = Vec::new();
    let mut found_keys = std::collections::HashSet::new();
    let mut builder = Config::builder();

    if path.exists() {
        builder = builder.add_source(File::from(path).format(FileFormat::Ini));
    }

    let config = match builder.build() {
        Ok(cfg) => cfg,
        Err(_) => {
            for schema in schema_list {
                entries.push(ConfigEntry {
                    key: schema.key.clone(),
                    value: schema.default.clone(),
                    schema: Some(schema),
                    enabled: false,
                });
            }
            return Ok(entries);
        }
    };

    let mut config_keys = HashSet::new();

    let sections = vec!["", "main", "test", "signet", "regtest"];

    for section in &sections {
        if let Ok(table) = if section.is_empty() {
            config.get_table("")
        } else {
            config.get_table(section)
        } {
            for key in table.keys() {
                let actual_key = if key.contains('.') {
                    key.split('.').next_back().unwrap_or(key).to_string()
                } else {
                    key.clone()
                };
                config_keys.insert(actual_key);
            }
        }
    }

    for schema in &schema_list {
        let key = &schema.key;
        let mut value = schema.default.clone();
        let mut enabled = false;

        for section in &sections {
            let lookup_key = if section.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", section, key)
            };

            if let Ok(val) = config.get_string(&lookup_key) {
                value = val;
                enabled = true;
                found_keys.insert(key.clone());
                break;
            }

            if let Ok(val) = config.get_bool(&lookup_key) {
                value = if val {
                    "1".to_string()
                } else {
                    "0".to_string()
                };
                enabled = true;
                found_keys.insert(key.clone());
                break;
            }

            if let Ok(val) = config.get_int(&lookup_key) {
                value = val.to_string();
                enabled = true;
                found_keys.insert(key.clone());
                break;
            }

            if let Ok(val) = config.get_float(&lookup_key) {
                value = val.to_string();
                enabled = true;
                found_keys.insert(key.clone());
                break;
            }
        }

        entries.push(ConfigEntry {
            key: key.clone(),
            value,
            schema: Some(schema.clone()),
            enabled,
        });
    }

    for config_key in &config_keys {
        if !found_keys.contains(config_key) {
            let value = config
                .get_string(config_key)
                .or_else(|_| {
                    config
                        .get_bool(config_key)
                        .map(|b| if b { "1".to_string() } else { "0".to_string() })
                })
                .or_else(|_| config.get_int(config_key).map(|i| i.to_string()))
                .or_else(|_| config.get_float(config_key).map(|f| f.to_string()))
                .unwrap_or_else(|_| "".to_string());

            entries.push(ConfigEntry {
                key: config_key.clone(),
                value,
                schema: None,
                enabled: true,
            });
        }
    }

    Ok(entries)
}
