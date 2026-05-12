// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::components::p2pool_websocket::P2PoolWebSocketClient;
use crate::config::load_api_config;
use reqwest::Client;
use serde::Deserialize;
use serde::Deserializer;
use serde::de::DeserializeOwned;
use std::time::Duration;

const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const TESTNET4_FALLBACK_BASE_URL: &str = "https://testnet4.p2poolv2.org";

#[derive(Debug, Clone)]
pub struct P2PoolClient {
    client: Client,
    base_url: String,
    fallback_base_url: Option<String>,
    auth_credentials: Option<(String, String)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainInfo {
    pub genesis_blockhash: Option<String>,
    pub chain_tip_height: Option<u64>,
    pub total_work: String,
    pub chain_tip_blockhash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShareInfo {
    pub blockhash: String,
    pub prev_blockhash: String,
    pub height: u64,
    pub miner_address: String,
    pub timestamp: u64,
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub bits: String,
    #[serde(default)]
    pub uncles: Vec<UncleInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UncleInfo {
    pub blockhash: String,
    pub prev_blockhash: String,
    pub miner_address: String,
    pub timestamp: u64,
    pub height: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SharesResponse {
    pub from_height: u64,
    pub to_height: u64,
    #[serde(default)]
    pub shares: Vec<ShareInfo>,
}

fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .build()
        .expect("Failed to build reqwest client")
}

impl P2PoolClient {
    pub fn new() -> Self {
        let (base_url, auth_credentials) = load_api_config()
            .map(|cfg| {
                let credentials = cfg.auth_user.zip(cfg.auth_pass);
                (cfg.base_url, credentials)
            })
            .unwrap_or_else(|_| (TESTNET4_FALLBACK_BASE_URL.to_string(), None));

        let fallback_base_url = (base_url != TESTNET4_FALLBACK_BASE_URL)
            .then(|| TESTNET4_FALLBACK_BASE_URL.to_string());

        Self {
            client: build_client(),
            base_url,
            fallback_base_url,
            auth_credentials,
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: build_client(),
            base_url: base_url.into(),
            fallback_base_url: None,
            auth_credentials: None,
        }
    }

    pub fn with_client(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            fallback_base_url: None,
            auth_credentials: None,
        }
    }

    pub fn with_auth(mut self, user: String, pass: String) -> Self {
        self.auth_credentials = Some((user, pass));
        self
    }

    pub fn with_fallback_base_url(mut self, fallback_base_url: impl Into<String>) -> Self {
        self.fallback_base_url = Some(fallback_base_url.into());
        self
    }

    pub fn websocket_client(&self) -> P2PoolWebSocketClient {
        let mut client = P2PoolWebSocketClient::with_base_url(self.base_url.clone());
        if let Some((user, pass)) = &self.auth_credentials {
            client = client.with_auth(user.clone(), pass.clone());
        }
        if let Some(fallback_base_url) = &self.fallback_base_url {
            client = client.with_fallback_base_url(fallback_base_url.clone());
        }
        client
    }

    pub async fn fetch_chain_info(&self) -> Result<ChainInfo, reqwest::Error> {
        self.fetch_json_with_fallback("/chain_info", &[]).await
    }

    pub async fn fetch_peer_info(&self) -> Result<Vec<PeerInfo>, reqwest::Error> {
        self.fetch_json_with_fallback("/peers", &[]).await
    }

    pub async fn fetch_recent_shares(&self, num: u16) -> Result<SharesResponse, reqwest::Error> {
        self.fetch_json_with_fallback("/shares", &[("num", num.min(100))])
            .await
    }

    async fn fetch_json_with_fallback<T>(
        &self,
        path: &str,
        query: &[(&str, u16)],
    ) -> Result<T, reqwest::Error>
    where
        T: DeserializeOwned,
    {
        match self
            .fetch_json_from_base_url(&self.base_url, path, query, true)
            .await
        {
            Ok(data) => Ok(data),
            Err(error) => {
                if self.should_try_fallback(&error) {
                    if let Some(fallback_base_url) = &self.fallback_base_url {
                        return self
                            .fetch_json_from_base_url(fallback_base_url, path, query, false)
                            .await;
                    }
                }
                Err(error)
            }
        }
    }

    async fn fetch_json_from_base_url<T>(
        &self,
        base_url: &str,
        path: &str,
        query: &[(&str, u16)],
        use_auth: bool,
    ) -> Result<T, reqwest::Error>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", base_url.trim_end_matches('/'), path);
        let mut request = self.client.get(url);

        if !query.is_empty() {
            request = request.query(query);
        }

        if use_auth {
            if let Some((user, pass)) = &self.auth_credentials {
                request = request.basic_auth(user, Some(pass));
            }
        }

        let response = request.send().await?.error_for_status()?;
        response.json::<T>().await
    }

    fn should_try_fallback(&self, error: &reqwest::Error) -> bool {
        self.fallback_base_url.is_some() && (error.is_connect() || error.is_timeout())
    }
}

impl Default for P2PoolClient {
    fn default() -> Self {
        Self::new()
    }
}

fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(u64),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(value) => Ok(value),
        StringOrNumber::Number(value) => Ok(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};
    use serde_json::json;

    #[test]
    fn explicit_base_url_does_not_enable_network_fallback() {
        let client = P2PoolClient::with_base_url("http://127.0.0.1:46884");

        assert_eq!(client.base_url, "http://127.0.0.1:46884");
        assert_eq!(client.fallback_base_url, None);
    }

    #[test]
    fn fallback_base_url_can_be_configured() {
        let client = P2PoolClient::with_base_url("http://127.0.0.1:46884")
            .with_fallback_base_url("https://testnet4.p2poolv2.org");

        assert_eq!(
            client.fallback_base_url.as_deref(),
            Some("https://testnet4.p2poolv2.org")
        );
    }

    #[tokio::test]
    async fn test_fetch_chain_info_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/chain_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "genesis_blockhash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
                    "chain_tip_height": 850_000u64,
                    "total_work": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "chain_tip_blockhash": "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72728a054"
                })
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.chain_tip_height, Some(850_000));
        assert_eq!(
            result.total_work,
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        );
        assert_eq!(
            result.genesis_blockhash.unwrap(),
            "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
        );
        assert_eq!(
            result.chain_tip_blockhash.unwrap(),
            "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72728a054"
        );
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_chain_info_sends_basic_auth() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/chain_info")
            .match_header("authorization", "Basic dXNlcjpwYXNzd29yZA==")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "total_work": "abc" }).to_string())
            .create();

        let client =
            P2PoolClient::with_base_url(server.url()).with_auth("user".into(), "password".into());

        client.fetch_chain_info().await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_peer_info_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/peers")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!([
                    {
                        "peer_id": "12D3KooWPeerOne",
                        "status": "Connected"
                    }
                ])
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let result = client.fetch_peer_info().await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].peer_id, "12D3KooWPeerOne");
        assert_eq!(result[0].status.as_deref(), Some("Connected"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_peer_info_accepts_missing_status() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/peers")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!([{ "peer_id": "12D3KooWPeerOne" }]).to_string())
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let result = client.fetch_peer_info().await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].peer_id, "12D3KooWPeerOne");
        assert_eq!(result[0].status, None);
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_peer_info_sends_basic_auth() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/peers")
            .match_header("authorization", "Basic dXNlcjpwYXNzd29yZA==")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!([]).to_string())
            .create();

        let client =
            P2PoolClient::with_base_url(server.url()).with_auth("user".into(), "password".into());

        client.fetch_peer_info().await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_recent_shares_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/shares")
            .match_query(Matcher::UrlEncoded("num".into(), "2".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "from_height": 41,
                    "to_height": 42,
                    "shares": [
                        {
                            "blockhash": "0000share",
                            "prev_blockhash": "ffffprev",
                            "height": 42,
                            "miner_address": "miner-address",
                            "timestamp": 1700000000u64,
                            "bits": "1d00ffff",
                            "uncles": [
                                {
                                    "blockhash": "0000uncle",
                                    "prev_blockhash": "ffffuncleprev",
                                    "miner_address": "uncle-miner",
                                    "timestamp": 1699999999u64,
                                    "height": 41
                                }
                            ]
                        }
                    ]
                })
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let result = client.fetch_recent_shares(2).await.unwrap();

        assert_eq!(result.from_height, 41);
        assert_eq!(result.to_height, 42);
        assert_eq!(result.shares.len(), 1);
        assert_eq!(result.shares[0].height, 42);
        assert_eq!(result.shares[0].uncles.len(), 1);
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_recent_shares_accepts_numeric_bits() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/shares")
            .match_query(Matcher::UrlEncoded("num".into(), "1".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "from_height": 42,
                    "to_height": 42,
                    "shares": [
                        {
                            "blockhash": "0000share",
                            "prev_blockhash": "ffffprev",
                            "height": 42,
                            "miner_address": "miner-address",
                            "timestamp": 1700000000u64,
                            "bits": 454130449,
                            "uncles": []
                        }
                    ]
                })
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let result = client.fetch_recent_shares(1).await.unwrap();

        assert_eq!(result.shares[0].bits, "454130449");
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_chain_info_errors_on_http_500() {
        let mut server = Server::new_async().await;

        server.mock("GET", "/chain_info").with_status(500).create();

        let client = P2PoolClient::with_base_url(server.url());
        assert!(client.fetch_chain_info().await.is_err());
    }

    #[tokio::test]
    async fn test_fetch_chain_info_returns_error_on_missing_required_field() {
        let mut server = Server::new_async().await;

        server
            .mock("GET", "/chain_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "chain_tip_height": 100 }).to_string())
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        assert!(client.fetch_chain_info().await.is_err());
    }

    #[tokio::test]
    async fn test_with_client_can_be_injected_for_isolated_tests() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/chain_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "genesis_blockhash": null,
                    "chain_tip_height": 1,
                    "total_work": "abc",
                    "chain_tip_blockhash": null
                })
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_client(build_client(), server.url());
        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.chain_tip_height, Some(1));
        assert_eq!(result.total_work, "abc");
        mock.assert();
    }
}
