// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::components::p2pool_websocket::P2PoolWebSocketClient;
use p2poolv2_config::{ApiConfig as P2PoolApiConfig, Config as P2PoolConfig};
use reqwest::Client;
use serde::Deserialize;
use serde::Deserializer;
use serde::de::DeserializeOwned;
use std::time::Duration;

const REQUEST_TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, Clone)]
pub struct P2PoolClient {
    client: Client,
    base_url: String,
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
        Self::with_base_url("")
    }

    pub fn from_p2pool_config(config: &P2PoolConfig) -> Self {
        Self::from_api_config(&config.api)
    }

    fn from_api_config(config: &P2PoolApiConfig) -> Self {
        let client =
            P2PoolClient::with_base_url(format!("http://{}:{}", config.hostname, config.port));

        if let Some((user, pass)) = api_auth_credentials(config) {
            client.with_auth(user, pass)
        } else {
            client
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: build_client(),
            base_url: base_url.into(),
            auth_credentials: None,
        }
    }

    pub fn with_client(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            auth_credentials: None,
        }
    }

    pub fn with_auth(mut self, user: String, pass: String) -> Self {
        self.auth_credentials = Some((user, pass));
        self
    }

    pub fn websocket_client(&self) -> P2PoolWebSocketClient {
        let mut client = P2PoolWebSocketClient::with_base_url(self.base_url.clone());
        if let Some((user, pass)) = &self.auth_credentials {
            client = client.with_auth(user.clone(), pass.clone());
        }

        client
    }

    pub async fn fetch_chain_info(&self) -> Result<ChainInfo, reqwest::Error> {
        self.fetch_json("/chain_info", &[]).await
    }

    pub async fn fetch_peer_info(&self) -> Result<Vec<PeerInfo>, reqwest::Error> {
        self.fetch_json("/peers", &[]).await
    }

    pub async fn fetch_recent_shares(&self, num: u16) -> Result<SharesResponse, reqwest::Error> {
        self.fetch_json("/shares", &[("num", num.min(100))]).await
    }

    async fn fetch_json<T>(&self, path: &str, query: &[(&str, u16)]) -> Result<T, reqwest::Error>
    where
        T: DeserializeOwned,
    {
        self.fetch_json_from_base_url(&self.base_url, path, query, true)
            .await
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

        if use_auth && let Some((user, pass)) = &self.auth_credentials {
            request = request.basic_auth(user, Some(pass));
        }

        let response = request.send().await?.error_for_status()?;
        response.json::<T>().await
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

fn api_auth_credentials(config: &P2PoolApiConfig) -> Option<(String, String)> {
    match (&config.auth_user, &config.auth_password) {
        (Some(user), Some(password)) if !user.is_empty() && !password.is_empty() => {
            Some((user.clone(), password.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};
    use serde_json::json;

    fn loaded_p2pool_config() -> P2PoolConfig {
        P2PoolConfig::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .expect("fixture config must load")
    }

    #[test]
    fn new_starts_without_pdm_side_api_defaults() {
        let client = P2PoolClient::new();

        assert_eq!(client.base_url, "");
        assert_eq!(client.auth_credentials, None);
    }
    #[test]
    fn from_p2pool_config_uses_api_hostname_and_port() {
        let mut config = loaded_p2pool_config();
        config.api.hostname = "192.0.2.10".to_string();
        config.api.port = 39001;

        let client = P2PoolClient::from_p2pool_config(&config);

        assert_eq!(client.base_url, "http://192.0.2.10:39001");
    }

    #[test]
    fn from_p2pool_config_uses_api_auth_password_for_basic_auth() {
        let mut config = loaded_p2pool_config();
        config.api.auth_user = Some("pdm-user".to_string());
        config.api.auth_token = Some("stored-token".to_string());
        config.api.auth_password = Some("pdm-pass".to_string());

        let client = P2PoolClient::from_p2pool_config(&config);

        assert_eq!(
            client.auth_credentials,
            Some(("pdm-user".to_string(), "pdm-pass".to_string()))
        );
    }

    #[test]
    fn from_p2pool_config_does_not_use_stored_auth_token_as_password() {
        let mut config = loaded_p2pool_config();
        config.api.auth_user = Some("pdm-user".to_string());
        config.api.auth_token = Some("stored-token".to_string());
        config.api.auth_password = None;

        let client = P2PoolClient::from_p2pool_config(&config);

        assert_eq!(client.auth_credentials, None);
    }

    #[tokio::test]
    async fn fetch_uses_p2pool_config_api_values() {
        let mut server = Server::new_async().await;
        let server_url = url::Url::parse(&server.url()).unwrap();
        let mut config = loaded_p2pool_config();
        config.api.hostname = server_url.host_str().unwrap().to_string();
        config.api.port = server_url.port().unwrap();
        config.api.auth_user = Some("user".to_string());
        config.api.auth_password = Some("password".to_string());

        let mock = server
            .mock("GET", "/chain_info")
            .match_header("authorization", "Basic dXNlcjpwYXNzd29yZA==")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "total_work": "abc" }).to_string())
            .create();

        let client = P2PoolClient::from_p2pool_config(&config);
        let result = client.fetch_chain_info().await.unwrap();

        assert_eq!(result.total_work, "abc");
        mock.assert();
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
