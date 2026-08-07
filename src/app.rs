// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::bitcoin_config::ConfigEntry as BitcoinEntry;
use crate::components::bitcoin_config_view::BitcoinConfigView;
use crate::components::file_explorer::FileExplorer;
use crate::components::p2pool_client::{ChainInfo, P2PoolClient, PeerInfo, SharesResponse};
use crate::components::p2pool_config_view::P2PoolConfigView;
use crate::components::p2pool_websocket::{
    LiveP2PoolEvent, LivePeerEvent, LiveShare, P2PoolWebSocketClient,
};
use crate::components::settings_view::SettingsView;
use crate::settings::Settings;
use p2poolv2_config::Config as P2PoolConfig;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Sidebar items labels
pub const SIDEBAR_ITEMS: &[(&str, CurrentScreen)] = &[
    ("Home", CurrentScreen::Home),
    ("Bitcoin Config", CurrentScreen::BitcoinConfig),
    ("Bitcoin Status", CurrentScreen::BitcoinStatus),
    ("P2Pool Config", CurrentScreen::P2PoolConfig),
    ("P2Pool Status", CurrentScreen::P2PoolStatus),
    ("LN Config", CurrentScreen::LNConfig),
    ("LN Status", CurrentScreen::LNStatus),
    ("Shares Market", CurrentScreen::SharesMarket),
    ("Settings", CurrentScreen::Settings),
];

pub const MAX_SIDEBAR_INDEX: usize = SIDEBAR_ITEMS.len() - 1;

/// Tab labels for the Bitcoin Status view
pub const BITCOIN_STATUS_TABS: &[&str] = &["Chain Info", "System", "Logs", "Peers"];

pub const MAX_BITCOIN_STATUS_TAB: usize = BITCOIN_STATUS_TABS.len() - 1;

/// Tab labels for the P2Pool Status view
pub const P2POOL_STATUS_TABS: &[&str] = &["Chain Info", "Shares", "Peers Info"];

pub const MAX_P2POOL_STATUS_TAB: usize = P2POOL_STATUS_TABS.len() - 1;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CurrentScreen {
    Home,
    BitcoinConfig,
    BitcoinStatus,
    P2PoolConfig,
    P2PoolStatus,
    LNConfig,
    LNStatus,
    SharesMarket,
    FileExplorer,
    Settings,
}

/// Identifies which screen (and optionally which field) triggered the file explorer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerTrigger {
    BitcoinConfig,
    P2PoolConfig,
    /// The `usize` is the settings field index (0–`FIELD_COUNT - 1`).
    Settings(usize),
}

/// Actions that components (Explorer, Editors) can trigger.
/// This decouples input handling from business logic.
#[derive(Debug, Clone)]
pub enum AppAction {
    None,
    Quit,
    ToggleMenu,
    Navigate(CurrentScreen),
    // Triggers the file explorer; the trigger identifies the caller
    OpenExplorer(ExplorerTrigger),
    // Returned by the Explorer when user picks a file
    FileSelected(PathBuf),
    // Closes the explorer without selection
    CloseModal,
    // Commits an edited value: (entry index, new value)
    CommitEdit(usize, String),
    // Saves bitcoin config to disk
    SaveBitcoinConfig,
    /// Commits an edited p2pool config value: (entry index, new value)
    CommitP2PoolEdit(usize, String),
    /// Saves p2pool config to disk
    SaveP2PoolConfig,
    // Open the file explorer to pick a path for a settings field (field index)
    OpenExplorerForSettings(usize),
    // Clear a settings field by index, setting it back to None
    ClearSettingsField(usize),
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub sidebar_index: usize,
    pub explorer_trigger: Option<ExplorerTrigger>,
    pub bitcoin_conf_path: Option<PathBuf>,
    pub p2pool_conf_path: Option<PathBuf>,
    pub explorer: FileExplorer,
    pub bitcoin_config_view: BitcoinConfigView,
    pub p2pool_config_view: P2PoolConfigView,
    pub settings_view: SettingsView,
    pub p2pool_config: Option<P2PoolConfig>,
    pub bitcoin_data: Vec<BitcoinEntry>,
    pub bitcoin_status_tab: usize,
    pub settings: Settings,
    pub p2pool_client: P2PoolClient,
    pub p2pool_websocket_client: P2PoolWebSocketClient,
    /// Cached value of the `HOME` environment variable, used for path display.
    /// Populated once at startup to avoid repeated syscalls during rendering.
    pub home_dir: String,
    /// Cached result of `settings::config_dir()`, used to display the default
    /// settings storage path without repeated env-var lookups during rendering.
    pub config_dir: PathBuf,
    pub p2pool_status_tab: usize,
    pub chain_info: Option<ChainInfo>,
    pub p2pool_chain_info_error: Option<String>,
    pub share_info: Option<SharesResponse>,
    pub p2pool_share_info_error: Option<String>,
    pub peer_info: Option<Vec<PeerInfo>>,
    pub p2pool_peer_info_error: Option<String>,
    pub live_shares: Vec<LiveShare>,
    pub live_peer_events: Vec<LivePeerEvent>,
    pub p2pool_live_error: Option<String>,
    pub p2pool_live_stream_started: bool,
    pub p2pool_live_tx: mpsc::UnboundedSender<anyhow::Result<LiveP2PoolEvent>>,
    pub p2pool_live_rx: mpsc::UnboundedReceiver<anyhow::Result<LiveP2PoolEvent>>,
    // async channel to receive chain info updates from the background task that
    // fetches it when the P2Pool Status screen is opened.
    pub chain_info_tx: mpsc::UnboundedSender<anyhow::Result<ChainInfo>>,
    pub chain_info_rx: mpsc::UnboundedReceiver<anyhow::Result<ChainInfo>>,
    pub share_info_tx: mpsc::UnboundedSender<anyhow::Result<SharesResponse>>,
    pub share_info_rx: mpsc::UnboundedReceiver<anyhow::Result<SharesResponse>>,
    pub peer_info_tx: mpsc::UnboundedSender<anyhow::Result<Vec<PeerInfo>>>,
    pub peer_info_rx: mpsc::UnboundedReceiver<anyhow::Result<Vec<PeerInfo>>>,
}

impl App {
    #[must_use]
    pub fn new() -> App {
        let (chain_info_tx, chain_info_rx) = mpsc::unbounded_channel();
        let (peer_info_tx, peer_info_rx) = mpsc::unbounded_channel();
        let (share_info_tx, share_info_rx) = mpsc::unbounded_channel();
        let (p2pool_live_tx, p2pool_live_rx) = mpsc::unbounded_channel();
        App {
            current_screen: CurrentScreen::Home,
            sidebar_index: 0,
            explorer_trigger: None,
            bitcoin_conf_path: None,
            p2pool_conf_path: None,
            explorer: FileExplorer::new(),
            bitcoin_config_view: BitcoinConfigView::new(),
            p2pool_config_view: P2PoolConfigView::new(),
            settings_view: SettingsView::new(),
            p2pool_config: None,
            bitcoin_data: Vec::new(),
            bitcoin_status_tab: 0,
            settings: Settings::default(),
            p2pool_client: P2PoolClient::new(),
            p2pool_websocket_client: P2PoolWebSocketClient::new(),
            home_dir: std::env::var("HOME").unwrap_or_default(),
            config_dir: crate::settings::config_dir().unwrap_or_default(),
            p2pool_status_tab: 0,
            chain_info: None,
            p2pool_chain_info_error: None,
            share_info: None,
            p2pool_share_info_error: None,
            peer_info: None,
            p2pool_peer_info_error: None,
            live_shares: Vec::new(),
            live_peer_events: Vec::new(),
            p2pool_live_error: None,
            p2pool_live_stream_started: false,
            p2pool_live_tx,
            p2pool_live_rx,
            chain_info_tx,
            chain_info_rx,
            share_info_tx,
            share_info_rx,
            peer_info_tx,
            peer_info_rx,
        }
    }

    #[must_use]
    pub fn new_with_client(client: P2PoolClient) -> App {
        let mut app = App::new();
        app.p2pool_websocket_client = client.websocket_client();
        app.p2pool_client = client;
        app
    }

    /// Non-blocking result handler
    pub fn poll_chain_info(&mut self) {
        while let Ok(result) = self.chain_info_rx.try_recv() {
            match result {
                Ok(info) => {
                    self.chain_info = Some(info);
                    self.p2pool_chain_info_error = None;
                }
                Err(e) => {
                    self.chain_info = None;
                    self.p2pool_chain_info_error = Some(e.to_string());
                }
            }
        }
    }

    pub fn poll_peer_info(&mut self) {
        while let Ok(result) = self.peer_info_rx.try_recv() {
            match result {
                Ok(info) => {
                    self.peer_info = Some(info);
                    self.p2pool_peer_info_error = None;
                }
                Err(e) => {
                    self.peer_info = None;
                    self.p2pool_peer_info_error = Some(e.to_string());
                }
            }
        }
    }

    pub fn poll_share_info(&mut self) {
        while let Ok(result) = self.share_info_rx.try_recv() {
            match result {
                Ok(info) => {
                    self.share_info = Some(info);
                    self.p2pool_share_info_error = None;
                }
                Err(e) => {
                    self.share_info = None;
                    self.p2pool_share_info_error = Some(e.to_string());
                }
            }
        }
    }

    pub fn poll_live_p2pool_events(&mut self) {
        while let Ok(result) = self.p2pool_live_rx.try_recv() {
            match result {
                Ok(LiveP2PoolEvent::Share(share)) => {
                    Self::push_limited(&mut self.live_shares, share, 50);
                    self.p2pool_live_error = None;
                }
                Ok(LiveP2PoolEvent::Peer(peer_event)) => {
                    self.apply_live_peer_event(&peer_event);
                    Self::push_limited(&mut self.live_peer_events, peer_event, 50);
                    self.p2pool_live_error = None;
                }
                Err(e) => {
                    self.p2pool_live_error = Some(e.to_string());
                    self.p2pool_live_stream_started = false;
                }
            }
        }
    }

    pub fn poll_live_shares(&mut self) {
        self.poll_live_p2pool_events();
    }

    fn push_limited<T>(items: &mut Vec<T>, item: T, max_len: usize) {
        items.push(item);
        if items.len() > max_len {
            let extra = items.len() - max_len;
            items.drain(0..extra);
        }
    }

    fn apply_live_peer_event(&mut self, event: &LivePeerEvent) {
        if event.status.eq_ignore_ascii_case("disconnected") {
            if let Some(peers) = &mut self.peer_info {
                peers.retain(|peer| peer.peer_id != event.peer_id);
            }
            return;
        }

        let peers = self.peer_info.get_or_insert_with(Vec::new);
        if let Some(peer) = peers.iter_mut().find(|peer| peer.peer_id == event.peer_id) {
            peer.status = Some(event.status.clone());
        } else {
            peers.push(PeerInfo {
                peer_id: event.peer_id.clone(),
                status: Some(event.status.clone()),
            });
        }
    }

    // Logic to switch between sidebar items
    pub fn toggle_menu(&mut self) {
        if self.current_screen == CurrentScreen::BitcoinConfig {
            self.bitcoin_config_view.warning_message = None;
            self.bitcoin_config_view.save_message = None;
            self.bitcoin_config_view.editing = false;
            self.bitcoin_config_view.edit_input.clear();
        }
        if self.current_screen == CurrentScreen::P2PoolConfig {
            self.p2pool_config_view.warning_message = None;
            self.p2pool_config_view.save_message = None;
            self.p2pool_config_view.editing = false;
            self.p2pool_config_view.edit_input.clear();
        }
        if let Some(&(_, screen)) = SIDEBAR_ITEMS.get(self.sidebar_index) {
            self.current_screen = screen;
            if self.current_screen == CurrentScreen::P2PoolStatus {
                let chain_client = self.p2pool_client.clone();
                let chain_tx = self.chain_info_tx.clone();
                let share_client = self.p2pool_client.clone();
                let share_tx = self.share_info_tx.clone();
                let peer_client = self.p2pool_client.clone();
                let peer_tx = self.peer_info_tx.clone();
                let websocket_client = self.p2pool_websocket_client.clone();
                let live_tx = self.p2pool_live_tx.clone();
                let start_live_stream = !self.p2pool_live_stream_started;

                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let res = chain_client.fetch_chain_info().await;
                        let _ = chain_tx.send(res.map_err(anyhow::Error::from));
                    });

                    handle.spawn(async move {
                        let res = share_client.fetch_recent_shares(10).await;
                        let _ = share_tx.send(res.map_err(anyhow::Error::from));
                    });

                    handle.spawn(async move {
                        let res = peer_client.fetch_peer_info().await;
                        let _ = peer_tx.send(res.map_err(anyhow::Error::from));
                    });

                    if start_live_stream {
                        self.p2pool_live_stream_started = true;
                        handle.spawn(async move {
                            if let Err(error) = websocket_client
                                .subscribe_live_events(live_tx.clone())
                                .await
                            {
                                let _ = live_tx.send(Err(error));
                            }
                        });
                    }
                }
            }
        }
    }
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
