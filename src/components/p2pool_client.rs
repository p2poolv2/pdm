// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::config::load_api_config;
use reqwest::Client;
use serde::Deserialize;
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
            .unwrap_or_else(|_| ("http://localhost:46884".to_string(), None));

        Self {
            client: build_client(),
            base_url,
            auth_credentials,
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

    pub async fn fetch_chain_info(&self) -> Result<ChainInfo, reqwest::Error> {
        let url = format!("{}/chain_info", self.base_url);
        let mut request = self.client.get(url);

        if let Some((user, pass)) = &self.auth_credentials {
            request = request.basic_auth(user, Some(pass));
        }

        let response = request.send().await?.error_for_status()?;
        let data = response.json::<ChainInfo>().await?;

        Ok(data)
    }
}

impl Default for P2PoolClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

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
