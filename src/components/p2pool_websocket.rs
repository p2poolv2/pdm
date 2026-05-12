// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::config::load_api_config;
use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Deserializer};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const TESTNET4_FALLBACK_BASE_URL: &str = "https://testnet4.p2poolv2.org";

#[derive(Debug, Clone)]
pub struct P2PoolWebSocketClient {
    base_url: String,
    fallback_base_url: Option<String>,
    auth_credentials: Option<(String, String)>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ShareEventData {
    pub blockhash: String,
    pub prev_blockhash: String,
    pub height: u64,
    pub miner_address: String,
    pub timestamp: u64,
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub bits: String,
    #[serde(default)]
    pub uncles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PeerEventData {
    pub peer_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "topic", content = "data")]
pub enum WebSocketEvent {
    #[serde(rename = "Share")]
    Share(ShareEventData),
    #[serde(rename = "Peer")]
    Peer(PeerEventData),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveShare {
    pub blockhash: String,
    pub prev_blockhash: String,
    pub height: u64,
    pub miner_address: String,
    pub timestamp: u64,
    pub bits: String,
    pub uncles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePeerEvent {
    pub peer_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveP2PoolEvent {
    Share(LiveShare),
    Peer(LivePeerEvent),
}

impl P2PoolWebSocketClient {
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
            base_url,
            fallback_base_url,
            auth_credentials,
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
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

    fn ws_url(&self, path: &str) -> Result<Url> {
        self.ws_url_from_base_url(&self.base_url, path)
    }

    fn ws_url_from_base_url(&self, base_url: &str, path: &str) -> Result<Url> {
        let mut url = Url::parse(base_url)
            .with_context(|| format!("Failed to parse base URL: {base_url}"))?;

        match url.scheme() {
            "http" => url.set_scheme("ws").unwrap(),
            "https" => url.set_scheme("wss").unwrap(),
            _ => {}
        }

        url.set_path(path);
        Ok(url)
    }

    fn ws_url_with_auth(&self, path: &str) -> Result<Url> {
        let mut url = self.ws_url(path)?;
        if let Some((user, pass)) = &self.auth_credentials {
            let token = STANDARD.encode(format!("{}:{}", user, pass));
            url.query_pairs_mut().append_pair("token", &token);
        }
        Ok(url)
    }

    pub async fn subscribe_live_events(
        &self,
        tx: mpsc::UnboundedSender<anyhow::Result<LiveP2PoolEvent>>,
    ) -> anyhow::Result<()> {
        let url = self.ws_url_with_auth("/ws")?;
        match self.subscribe_live_events_at(url, tx.clone()).await {
            Ok(()) => Ok(()),
            Err(primary_error) => {
                if let Some(fallback_base_url) = &self.fallback_base_url {
                    let fallback_url = self.ws_url_from_base_url(fallback_base_url, "/ws")?;
                    if self
                        .subscribe_live_events_at(fallback_url, tx)
                        .await
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
                Err(primary_error)
            }
        }
    }

    async fn subscribe_live_events_at(
        &self,
        url: Url,
        tx: mpsc::UnboundedSender<anyhow::Result<LiveP2PoolEvent>>,
    ) -> anyhow::Result<()> {
        let (stream, _) = connect_async(url.as_str()).await?;
        let (mut write, mut read) = stream.split();

        for topic in ["shares", "peers"] {
            let subscribe_message = serde_json::json!({
                "action": "subscribe",
                "topic": topic,
            })
            .to_string();
            write.send(Message::Text(subscribe_message)).await?;
        }

        while let Some(message_result) = read.next().await {
            let message = message_result?;
            if let Message::Text(text) = message {
                match serde_json::from_str::<WebSocketEvent>(&text) {
                    Ok(WebSocketEvent::Share(data)) => {
                        let live_share = LiveShare {
                            blockhash: data.blockhash,
                            prev_blockhash: data.prev_blockhash,
                            height: data.height,
                            miner_address: data.miner_address,
                            timestamp: data.timestamp,
                            bits: data.bits,
                            uncles: data.uncles,
                        };
                        let _ = tx.send(Ok(LiveP2PoolEvent::Share(live_share)));
                    }
                    Ok(WebSocketEvent::Peer(data)) => {
                        let live_peer = LivePeerEvent {
                            peer_id: data.peer_id,
                            status: data.status,
                        };
                        let _ = tx.send(Ok(LiveP2PoolEvent::Peer(live_peer)));
                    }
                    Err(error) => {
                        let _ = tx.send(Err(anyhow::Error::new(error)));
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for P2PoolWebSocketClient {
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

    #[test]
    fn ws_url_converts_http_to_ws_and_encodes_auth_token() {
        let client = P2PoolWebSocketClient::with_base_url("http://127.0.0.1:46884")
            .with_auth("user".into(), "password".into());

        let url = client.ws_url_with_auth("/ws").unwrap();

        assert_eq!(
            url.as_str(),
            "ws://127.0.0.1:46884/ws?token=dXNlcjpwYXNzd29yZA%3D%3D"
        );
    }

    #[test]
    fn ws_url_converts_https_fallback_to_wss() {
        let client = P2PoolWebSocketClient::with_base_url("https://testnet4.p2poolv2.org");

        let url = client.ws_url("/ws").unwrap();

        assert_eq!(url.as_str(), "wss://testnet4.p2poolv2.org/ws");
    }

    #[test]
    fn websocket_event_accepts_share_messages() {
        let event: WebSocketEvent = serde_json::from_value(serde_json::json!({
            "topic": "Share",
            "data": {
                "blockhash": "0000",
                "prev_blockhash": "ffff",
                "height": 42,
                "miner_address": "miner",
                "timestamp": 1700000000,
                "bits": "1d00ffff",
                "uncles": ["aaaa"]
            }
        }))
        .unwrap();

        assert!(matches!(event, WebSocketEvent::Share(_)));
    }

    #[test]
    fn websocket_event_accepts_numeric_bits() {
        let event: WebSocketEvent = serde_json::from_value(serde_json::json!({
            "topic": "Share",
            "data": {
                "blockhash": "0000",
                "prev_blockhash": "ffff",
                "height": 42,
                "miner_address": "miner",
                "timestamp": 1700000000,
                "bits": 454130449,
                "uncles": []
            }
        }))
        .unwrap();

        let WebSocketEvent::Share(data) = event else {
            panic!("expected share event");
        };
        assert_eq!(data.bits, "454130449");
    }

    #[test]
    fn websocket_event_accepts_peer_messages() {
        let event: WebSocketEvent = serde_json::from_value(serde_json::json!({
            "topic": "Peer",
            "data": {
                "peer_id": "12D3KooWPeerOne",
                "status": "Connected"
            }
        }))
        .unwrap();

        assert!(matches!(event, WebSocketEvent::Peer(_)));
    }
}
