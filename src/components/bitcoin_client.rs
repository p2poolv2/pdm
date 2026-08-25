// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result, anyhow, bail};
use p2poolv2_config::Config as P2PoolConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::time::Duration;

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
    pub connected_peer_addresses: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BlockchainInfoResponse {
    chain: String,
    blocks: u64,
    bestblockhash: String,
    verificationprogress: Option<f64>,
    initialblockdownload: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PeerInfoResponse {
    addr: Option<String>,
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
    pub fn from_p2pool_config(config: &P2PoolConfig) -> Self {
        Self {
            client: build_client(),
            url: config.bitcoinrpc.url.clone(),
            auth_credentials: Some((
                config.bitcoinrpc.username.clone(),
                config.bitcoinrpc.password.clone(),
            )),
        }
    }

    pub async fn fetch_chain_info(&self) -> Result<BitcoinChainInfo> {
        let chain_info: BlockchainInfoResponse = self.rpc_call("getblockchaininfo").await?;
        let connection_count = self.rpc_call("getconnectioncount").await.ok();
        let connected_peer_addresses = self.fetch_connected_peer_addresses().await?;

        Ok(BitcoinChainInfo {
            network: display_network(&chain_info.chain).to_string(),
            block_height: chain_info.blocks,
            best_block_hash: chain_info.bestblockhash,
            verification_progress: chain_info.verificationprogress,
            initial_block_download: chain_info.initialblockdownload,
            connection_count,
            connected_peer_addresses,
        })
    }

    async fn fetch_connected_peer_addresses(&self) -> Result<Vec<String>> {
        let peers: Vec<PeerInfoResponse> = self.rpc_call("getpeerinfo").await?;

        Ok(peers
            .into_iter()
            .filter_map(|peer| {
                let address = peer.addr?.trim().to_string();
                (!address.is_empty()).then_some(address)
            })
            .collect())
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

    #[test]
    fn uses_p2pool_bitcoinrpc_configuration() {
        let config = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        let client = BitcoinClient::from_p2pool_config(&config);

        assert_eq!(client.url, config.bitcoinrpc.url);
        assert_eq!(
            client.auth_credentials,
            Some((config.bitcoinrpc.username, config.bitcoinrpc.password))
        );
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
        let peers_mock = server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getpeerinfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": [
                        { "addr": "192.0.2.1:8333" },
                        { "addr": "203.0.113.5:8333" }
                    ],
                    "error": null,
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
        assert_eq!(
            result.connected_peer_addresses,
            vec!["192.0.2.1:8333".to_string(), "203.0.113.5:8333".to_string(),]
        );
        chain_mock.assert();
        connections_mock.assert();
        peers_mock.assert();
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
        server
            .mock("POST", "/")
            .match_body(Matcher::Regex("getpeerinfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": [],
                    "error": null,
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
        server
            .mock("POST", "/")
            .match_header("authorization", "Basic YWxpY2U6c2VjcmV0")
            .match_body(Matcher::Regex("getconnectioncount".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": 8,
                    "error": null,
                    "id": "pdm"
                })
                .to_string(),
            )
            .create();

        server
            .mock("POST", "/")
            .match_header("authorization", "Basic YWxpY2U6c2VjcmV0")
            .match_body(Matcher::Regex("getpeerinfo".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "result": [],
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
}
