// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::components::bitcoin_config_view::BitcoinConfigView;
use crate::components::file_explorer::FileExplorer;
use crate::config::ConfigEntry as BitcoinEntry;
use p2poolv2_config::Config as P2PoolConfig;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Clone)]
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
    Exiting,
}

/// Actions that components (Explorer, Editors) can trigger.
/// This decouples input handling from business logic.
#[derive(Debug, Clone)]
pub enum AppAction {
    None,
    Quit,
    ToggleMenu,
    Navigate(CurrentScreen),
    // Triggers the file explorer for a specific screen
    OpenExplorer(CurrentScreen),
    // Returned by the Explorer when user picks a file
    FileSelected(PathBuf),
    // Closes the explorer without selection
    CloseModal,
    // Save the bitcoin config to file
    SaveConfig,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub sidebar_index: usize,
    pub explorer_trigger: Option<CurrentScreen>,
    pub bitcoin_conf_path: Option<PathBuf>,
    pub p2pool_conf_path: Option<PathBuf>,
    pub explorer: FileExplorer,
    pub p2pool_config: Option<P2PoolConfig>,
    pub bitcoin_data: Vec<BitcoinEntry>,
    pub bitcoin_config_view: BitcoinConfigView,
    pub bitcoin_status_tab: usize,
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::Home,
            sidebar_index: 0,
            explorer_trigger: None,
            bitcoin_conf_path: None,
            p2pool_conf_path: None,
            explorer: FileExplorer::new(),
            p2pool_config: None,
            bitcoin_data: Vec::new(),
            bitcoin_config_view: BitcoinConfigView::new(),
            bitcoin_status_tab: 0,
        }
    }

    pub fn toggle_menu(&mut self) {
        // Logic to switch between sidebar items
        match self.sidebar_index {
            0 => self.current_screen = CurrentScreen::Home,
            1 => self.current_screen = CurrentScreen::BitcoinConfig,
            2 => self.current_screen = CurrentScreen::BitcoinStatus,
            3 => self.current_screen = CurrentScreen::P2PoolConfig,
            4 => self.current_screen = CurrentScreen::P2PoolStatus,
            5 => self.current_screen = CurrentScreen::LNConfig,
            6 => self.current_screen = CurrentScreen::LNStatus,
            7 => self.current_screen = CurrentScreen::SharesMarket,
            _ => {}
        }
    }
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
