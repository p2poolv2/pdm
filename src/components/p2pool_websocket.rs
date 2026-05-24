// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::config::{ApiConfig, load_api_config};
use crate::config::{ApiConfig, load_api_config};
use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Deserializer};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

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
        Self::from_config(load_api_config().unwrap_or_default())
    }

    fn from_config(config: ApiConfig) -> Self {
        let client = P2PoolWebSocketClient::with_base_url(&config.base_url);

        let client = if let Some(fallback) = &config.fallback_base_url {
            client.with_fallback_base_url(fallback)
        } else {
            client
        };

        if let Some((user, pass)) = config.auth_user.zip(config.auth_pass) {
            client.with_auth(user, pass)
        } else {
            client
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

        let base_path = url.path().trim_end_matches('/');
        let extra_path = path.trim_start_matches('/');
        let full_path = if base_path.is_empty() || base_path == "/" {
            format!("/{extra_path}")
        } else {
            format!("{base_path}/{extra_path}")
        };
        url.set_path(&full_path);
        Ok(url)
    }

    fn ws_url_with_auth(&self, path: &str) -> Result<Url> {
        let mut url = self.ws_url(path)?;
        self.apply_auth(&mut url);
        Ok(url)
    }

    fn ws_urls_with_auth(&self, path: &str) -> Result<Vec<Url>> {
        let mut urls = vec![self.ws_url_with_auth(path)?];
        if urls[0].scheme() == "ws" {
            let mut wss_url = self.ws_url(path)?;
            self.apply_auth(&mut wss_url);
            wss_url.set_scheme("wss").unwrap();
            urls.push(wss_url);
        }
        Ok(urls)
    }

    fn apply_auth(&self, url: &mut Url) {
        if let Some((user, pass)) = &self.auth_credentials {
            let token = STANDARD.encode(format!("{}:{}", user, pass));
            url.query_pairs_mut().append_pair("token", &token);
        }
    }

    pub async fn subscribe_live_events(
        &self,
        tx: mpsc::UnboundedSender<anyhow::Result<LiveP2PoolEvent>>,
    ) -> anyhow::Result<()> {
        let urls = self.ws_urls_with_auth("/ws")?;
        let mut primary_error = None;

        for url in urls {
            match self.subscribe_live_events_at(url, tx.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if primary_error.is_none() {
                        primary_error = Some(error);
                    }
                }
            }
        }

        if let Some(fallback_base_url) = &self.fallback_base_url {
            let mut fallback_url = self.ws_url_from_base_url(fallback_base_url, "/ws")?;
            self.apply_auth(&mut fallback_url);
            if self
                .subscribe_live_events_at(fallback_url, tx)
                .await
                .is_ok()
            {
                return Ok(());
            }
        }

        Err(primary_error.unwrap_or_else(|| anyhow::anyhow!("websocket connection failed")))
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
            match message_result {
                Ok(message) => {
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
                Err(error) => {
                    let _ = tx.send(Err(anyhow::Error::new(error)));
                    return Ok(());
                }
            }
        }

        let _ = tx.send(Err(anyhow::anyhow!("websocket connection closed")));
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
    use futures_util::StreamExt;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

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
        let client = P2PoolWebSocketClient::with_base_url("https://127.0.0.1:46884");

        let url = client.ws_url("/ws").unwrap();

        assert_eq!(url.as_str(), "wss://127.0.0.1:46884/ws");
    }

    #[test]
    fn ws_url_appends_path_to_existing_base_path() {
        let client = P2PoolWebSocketClient::with_base_url("http://127.0.0.1:46884/api");

        let url = client.ws_url("/ws").unwrap();

        assert_eq!(url.as_str(), "ws://127.0.0.1:46884/api/ws");
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

    #[test]
    fn ws_urls_with_auth_try_plain_and_secure_websocket_schemes() {
        let client = P2PoolWebSocketClient::with_base_url("http://127.0.0.1:46884")
            .with_auth("user".into(), "password".into());

        let urls = client.ws_urls_with_auth("/ws").unwrap();

        assert_eq!(urls.len(), 2);
        assert_eq!(
            urls[0].as_str(),
            "ws://127.0.0.1:46884/ws?token=dXNlcjpwYXNzd29yZA%3D%3D"
        );
        assert_eq!(
            urls[1].as_str(),
            "wss://127.0.0.1:46884/ws?token=dXNlcjpwYXNzd29yZA%3D%3D"
        );
    }

    #[tokio::test]
    async fn subscribe_live_events_emits_share_and_peer_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket = accept_async(stream).await.unwrap();
            let (mut write, mut read) = websocket.split();

            for _ in 0..2 {
                let _ = read.next().await.unwrap().unwrap();
            }

            write.send(Message::Binary(vec![1, 2, 3])).await.unwrap();
            write
                .send(Message::Text(
                    serde_json::json!({
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
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            write
                .send(Message::Text(
                    serde_json::json!({
                        "topic": "Peer",
                        "data": {
                            "peer_id": "12D3KooWPeerOne",
                            "status": "Connected"
                        }
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            let _ = write.close().await;
        });

        let client = P2PoolWebSocketClient::with_base_url(format!("http://{addr}"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let subscribe_handle = tokio::spawn(async move { client.subscribe_live_events(tx).await });

        let first_event = rx.recv().await.unwrap().unwrap();
        let second_event = rx.recv().await.unwrap().unwrap();
        let result = subscribe_handle.await.unwrap();
        server.await.unwrap();

        assert!(result.is_ok());
        assert!(matches!(first_event, LiveP2PoolEvent::Share(_)));
        assert!(matches!(second_event, LiveP2PoolEvent::Peer(_)));
    }

    #[tokio::test]
    async fn subscribe_live_events_emits_error_when_connection_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket = accept_async(stream).await.unwrap();
            let (mut write, mut read) = websocket.split();

            let _ = read.next().await.unwrap().unwrap();
            let _ = read.next().await.unwrap().unwrap();
            let _ = write.close().await;
        });

        let client = P2PoolWebSocketClient::with_base_url(format!("http://{addr}"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let subscribe_handle = tokio::spawn(async move { client.subscribe_live_events(tx).await });

        let event = rx.recv().await.unwrap();
        let result = subscribe_handle.await.unwrap();
        server.await.unwrap();

        assert!(event.is_err());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn subscribe_live_events_emits_error_for_invalid_json() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket = accept_async(stream).await.unwrap();
            let (mut write, mut read) = websocket.split();

            let _ = read.next().await.unwrap().unwrap();
            let _ = read.next().await.unwrap().unwrap();
            write.send(Message::Text("not-json".into())).await.unwrap();
            let _ = write.close().await;
        });

        let client = P2PoolWebSocketClient::with_base_url(format!("http://{addr}"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let subscribe_handle = tokio::spawn(async move { client.subscribe_live_events(tx).await });

        let event = rx.recv().await.unwrap();
        let result = subscribe_handle.await.unwrap();
        server.await.unwrap();

        assert!(result.is_ok());
        assert!(event.is_err());
    }

    #[tokio::test]
    async fn subscribe_live_events_uses_fallback_base_url_on_connection_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fallback_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket = accept_async(stream).await.unwrap();
            let (mut write, mut read) = websocket.split();

            let _ = read.next().await.unwrap().unwrap();
            let _ = read.next().await.unwrap().unwrap();
            write
                .send(Message::Text(
                    serde_json::json!({
                        "topic": "Share",
                        "data": {
                            "blockhash": "fallback",
                            "prev_blockhash": "prev",
                            "height": 7,
                            "miner_address": "miner",
                            "timestamp": 1700000000,
                            "bits": "1d00ffff",
                            "uncles": []
                        }
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            let _ = write.close().await;
        });

        let client = P2PoolWebSocketClient::with_base_url("http://127.0.0.1:1")
            .with_fallback_base_url(format!("http://{fallback_addr}"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let subscribe_handle = tokio::spawn(async move { client.subscribe_live_events(tx).await });

        let event = rx.recv().await.unwrap().unwrap();
        let result = subscribe_handle.await.unwrap();
        server.await.unwrap();

        assert!(result.is_ok());
        assert!(matches!(event, LiveP2PoolEvent::Share(_)));
    }
}
