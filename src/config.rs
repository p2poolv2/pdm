use color_eyre::Result;
use std::fs;
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

pub fn get_default_schema() -> Vec<ConfigSchema> {
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

pub fn parse_config(path: &Path) -> Result<Vec<ConfigEntry>> {
    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let schema_list = get_default_schema();

    let mut entries = Vec::new();
    let mut found_keys = std::collections::HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '=').collect();
        let key = parts[0].trim().to_string();
        let value = if parts.len() > 1 {
            parts[1].trim().to_string()
        } else {
            "1".to_string()
        };

        let schema = schema_list.iter().find(|s| s.key == key).cloned();
        if schema.is_some() {
            found_keys.insert(key.clone());
        }

        entries.push(ConfigEntry {
            key,
            value,
            schema,
            enabled: true,
        });
    }

    for schema in schema_list {
        if !found_keys.contains(&schema.key) {
            entries.push(ConfigEntry {
                key: schema.key.clone(),
                value: schema.default.clone(),
                schema: Some(schema),
                enabled: false,
            });
        }
    }

    Ok(entries)
}
