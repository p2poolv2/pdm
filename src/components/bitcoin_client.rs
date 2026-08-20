// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::bitcoin_config::ConfigEntry;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

const REQUEST_TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, Clone)]
pub struct BitcoinClient {
    client: Client,
    url: String,
    auth_credentials: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitcoinChainInfo {
    pub network: String,
    pub block_height: u64,
    pub best_block_hash: String,
    pub verification_progress: Option<f64>,
    pub initial_block_download: Option<bool>,
    pub connection_count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Testnet4,
    Signet,
    Regtest,
}

#[derive(Debug, Deserialize)]
struct BlockchainInfoResponse {
    chain: String,
    blocks: u64,
    bestblockhash: String,
    verificationprogress: Option<f64>,
    initialblockdownload: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: &'static str,
    method: &'a str,
    params: &'static [Value],
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

impl BitcoinClient {
    #[must_use]
    pub fn from_config_entries(entries: &[ConfigEntry]) -> Self {
        let network = network_from_entries(entries);
        let port = entry_value(entries, "rpcport")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| default_rpc_port(network));
        let host = entry_value(entries, "rpcbind").unwrap_or("127.0.0.1");
        let url = rpc_url(host, port);
        let auth_credentials = rpc_auth(entries, network);

        Self {
            client: build_client(),
            url,
            auth_credentials,
        }
    }

    pub async fn fetch_chain_info(&self) -> Result<BitcoinChainInfo> {
        let chain_info: BlockchainInfoResponse = self.rpc_call("getblockchaininfo").await?;
        let connection_count = self.rpc_call("getconnectioncount").await.ok();

        Ok(BitcoinChainInfo {
            network: display_network(&chain_info.chain).to_string(),
            block_height: chain_info.blocks,
            best_block_hash: chain_info.bestblockhash,
            verification_progress: chain_info.verificationprogress,
            initial_block_download: chain_info.initialblockdownload,
            connection_count,
        })
    }

    async fn rpc_call<T>(&self, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request = RpcRequest {
            jsonrpc: "1.0",
            id: "pdm",
            method,
            params: &[],
        };

        let mut builder = self.client.post(&self.url).json(&request);
        if let Some((user, pass)) = &self.auth_credentials {
            builder = builder.basic_auth(user, Some(pass));
        }

        let response = builder
            .send()
            .await
            .with_context(|| format!("could not connect to Bitcoin Core at {}", self.url))?
            .error_for_status()
            .context("Bitcoin Core RPC returned an HTTP error")?
            .json::<RpcResponse<T>>()
            .await
            .context("Bitcoin Core RPC returned an invalid response")?;

        if let Some(error) = response.error {
            bail!("Bitcoin Core RPC error {}: {}", error.code, error.message);
        }

        response
            .result
            .ok_or_else(|| anyhow!("Bitcoin Core RPC response did not include a result"))
    }
}

fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .build()
        .expect("Failed to build reqwest client")
}

fn entry_value<'a>(entries: &'a [ConfigEntry], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|entry| entry.enabled && entry.key == key && !entry.value.trim().is_empty())
        .map(|entry| entry.value.trim())
}

fn network_from_entries(entries: &[ConfigEntry]) -> BitcoinNetwork {
    if bool_entry(entries, "regtest") {
        return BitcoinNetwork::Regtest;
    }
    if bool_entry(entries, "signet") {
        return BitcoinNetwork::Signet;
    }
    if bool_entry(entries, "testnet4") {
        return BitcoinNetwork::Testnet4;
    }
    if bool_entry(entries, "testnet") {
        return BitcoinNetwork::Testnet;
    }

    match entry_value(entries, "chain")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "test" | "testnet" | "testnet3" => BitcoinNetwork::Testnet,
        "testnet4" => BitcoinNetwork::Testnet4,
        "signet" => BitcoinNetwork::Signet,
        "regtest" => BitcoinNetwork::Regtest,
        _ => BitcoinNetwork::Mainnet,
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

fn default_rpc_port(network: BitcoinNetwork) -> u16 {
    match network {
        BitcoinNetwork::Mainnet => 8332,
        BitcoinNetwork::Testnet => 18332,
        BitcoinNetwork::Testnet4 => 48332,
        BitcoinNetwork::Signet => 38332,
        BitcoinNetwork::Regtest => 18443,
    }
}

fn rpc_url(host: &str, port: u16) -> String {
    let host = host.trim().trim_matches('/');
    if host.starts_with("http://") || host.starts_with("https://") {
        return host.to_string();
    }
    if has_explicit_port(host) {
        return format!("http://{host}");
    }
    if host.contains(':') && !host.starts_with('[') {
        return format!("http://[{host}]:{port}");
    }
    format!("http://{host}:{port}")
}

fn has_explicit_port(host: &str) -> bool {
    if let Some(end_bracket) = host.find(']') {
        return host[end_bracket + 1..].starts_with(':');
    }

    host.matches(':').count() == 1
        && host
            .rsplit_once(':')
            .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
}

fn rpc_auth(entries: &[ConfigEntry], network: BitcoinNetwork) -> Option<(String, String)> {
    if let (Some(user), Some(pass)) = (
        entry_value(entries, "rpcuser"),
        entry_value(entries, "rpcpassword"),
    ) {
        return Some((user.to_string(), pass.to_string()));
    }

    read_cookie_auth(entries, network).ok()
}

fn read_cookie_auth(entries: &[ConfigEntry], network: BitcoinNetwork) -> Result<(String, String)> {
    let cookie_path = cookie_path(entries, network);
    let content = std::fs::read_to_string(&cookie_path)
        .with_context(|| format!("could not read RPC cookie at {}", cookie_path.display()))?;
    let (user, pass) = content
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow!("RPC cookie did not contain username and password"))?;

    Ok((user.to_string(), pass.to_string()))
}

fn cookie_path(entries: &[ConfigEntry], network: BitcoinNetwork) -> PathBuf {
    if let Some(path) = entry_value(entries, "rpccookiefile") {
        let configured = PathBuf::from(path);
        if configured.is_absolute() {
            return configured;
        }
        return data_dir(entries, network).join(configured);
    }

    data_dir(entries, network).join(".cookie")
}

fn data_dir(entries: &[ConfigEntry], network: BitcoinNetwork) -> PathBuf {
    let base = entry_value(entries, "datadir")
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .unwrap_or_default();

    match network {
        BitcoinNetwork::Mainnet => base,
        BitcoinNetwork::Testnet => base.join("testnet3"),
        BitcoinNetwork::Testnet4 => base.join("testnet4"),
        BitcoinNetwork::Signet => base.join("signet"),
        BitcoinNetwork::Regtest => base.join("regtest"),
    }
}

fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".bitcoin"))
}

fn display_network(chain: &str) -> &str {
    match chain {
        "main" => "mainnet",
        "test" | "testnet" | "testnet3" | "testnet4" => "testnet",
        "signet" => "signet",
        "regtest" => "regtest",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};
    use serde_json::json;

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
    fn builds_default_mainnet_endpoint() {
        let client = BitcoinClient::from_config_entries(&[]);

        assert_eq!(client.url, "http://127.0.0.1:8332");
    }

    #[test]
    fn uses_configured_rpc_port_and_auth() {
        let entries = vec![
            entry("rpcport", "18443"),
            entry("rpcuser", "alice"),
            entry("rpcpassword", "secret"),
        ];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(client.url, "http://127.0.0.1:18443");
        assert_eq!(
            client.auth_credentials,
            Some(("alice".to_string(), "secret".to_string()))
        );
    }

    #[test]
    fn detects_network_from_chain_setting() {
        let entries = vec![entry("chain", "testnet4")];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(client.url, "http://127.0.0.1:48332");
    }

    #[test]
    fn preserves_rpcbind_with_explicit_port() {
        let entries = vec![entry("rpcbind", "127.0.0.1:18443")];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(client.url, "http://127.0.0.1:18443");
    }

    #[tokio::test]
    async fn fetch_chain_info_success() {
        let mut server = Server::new_async().await;

        let chain_mock = server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": {
                        "chain": "main",
                        "blocks": 850_000u64,
                        "bestblockhash": "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72728a054",
                        "verificationprogress": 0.9999,
                        "initialblockdownload": false
                    },
                    "error": null,
                    "id": "pdm"
                })
                .to_string(),
            )
            .create();
        let connections_mock = server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getconnectioncount".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "result": 8u64, "error": null, "id": "pdm" }).to_string())
            .create();
        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.network, "mainnet");
        assert_eq!(result.block_height, 850_000);
        assert_eq!(
            result.best_block_hash,
            "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72728a054"
        );
        assert_eq!(result.verification_progress, Some(0.9999));
        assert_eq!(result.initial_block_download, Some(false));
        assert_eq!(result.connection_count, Some(8));
        chain_mock.assert();
        connections_mock.assert();
    }

    #[tokio::test]
    async fn fetch_chain_info_returns_error_for_rpc_error_response() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": null,
                    "error": {"code": -8, "message": "invalid parameter"},
                    "id": "pdm"
                })
                .to_string(),
            )
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let error = client.fetch_chain_info().await.unwrap_err();

        assert_eq!(
            error.to_string(),
            "Bitcoin Core RPC error -8: invalid parameter"
        );
    }

    #[tokio::test]
    async fn fetch_chain_info_returns_error_for_http_error_response() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(500)
            .with_body("internal error")
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let error = client.fetch_chain_info().await.unwrap_err();

        assert_eq!(error.to_string(), "Bitcoin Core RPC returned an HTTP error");
    }

    #[tokio::test]
    async fn fetch_chain_info_returns_error_for_invalid_json_response() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not-json")
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let error = client.fetch_chain_info().await.unwrap_err();

        assert_eq!(
            error.to_string(),
            "Bitcoin Core RPC returned an invalid response"
        );
    }

    #[tokio::test]
    async fn fetch_chain_info_returns_error_when_result_is_missing() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "result": null, "error": null, "id": "pdm" }).to_string())
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let error = client.fetch_chain_info().await.unwrap_err();

        assert_eq!(
            error.to_string(),
            "Bitcoin Core RPC response did not include a result"
        );
    }

    #[tokio::test]
    async fn fetch_chain_info_treats_connection_count_failure_as_none() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": {
                        "chain": "main",
                        "blocks": 111,
                        "bestblockhash": "abc",
                        "verificationprogress": 0.5,
                        "initialblockdownload": true
                    },
                    "error": null,
                    "id": "pdm"
                })
                .to_string(),
            )
            .create();
        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getconnectioncount".to_string()))
            .with_status(500)
            .with_body("boom")
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: None,
        };

        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.block_height, 111);
        assert_eq!(result.best_block_hash, "abc");
        assert_eq!(result.verification_progress, Some(0.5));
        assert_eq!(result.initial_block_download, Some(true));
        assert_eq!(result.connection_count, None);
    }

    #[tokio::test]
    async fn fetch_chain_info_sends_basic_auth_credentials() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/")
            .match_header("authorization", "Basic YWxpY2U6c2VjcmV0")
            .match_body(Matcher::Regex("getblockchaininfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": {
                        "chain": "main",
                        "blocks": 1,
                        "bestblockhash": "abc",
                        "verificationprogress": null,
                        "initialblockdownload": null
                    },
                    "error": null,
                    "id": "pdm"
                })
                .to_string(),
            )
            .create();

        let client = BitcoinClient {
            client: build_client(),
            url: server.url(),
            auth_credentials: Some(("alice".to_string(), "secret".to_string())),
        };

        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.block_height, 1);
    }

    #[test]
    fn ignores_disabled_and_whitespace_only_config_entries() {
        let entries = vec![
            entry("rpcport", "   "),
            ConfigEntry {
                key: "rpcport".to_string(),
                value: "18443".to_string(),
                schema: None,
                enabled: false,
                section: None,
            },
        ];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(client.url, "http://127.0.0.1:8332");
    }

    #[test]
    fn falls_back_to_cookie_auth_when_rpc_password_is_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pdm-bitcoin-client-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cookie_path = temp_dir.join(".cookie");
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(&cookie_path, "alice:secret").unwrap();

        let entries = vec![
            entry("rpcuser", "alice"),
            entry("rpccookiefile", cookie_path.to_string_lossy().as_ref()),
        ];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(
            client.auth_credentials,
            Some(("alice".to_string(), "secret".to_string()))
        );

        let _ = std::fs::remove_file(cookie_path);
        let _ = std::fs::remove_dir(temp_dir);
    }

    #[test]
    fn formats_ipv6_rpcbind_without_explicit_port() {
        let entries = vec![entry("rpcbind", "::1")];
        let client = BitcoinClient::from_config_entries(&entries);

        assert_eq!(client.url, "http://[::1]:8332");
    }
}
