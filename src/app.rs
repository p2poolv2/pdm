// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::components::config_editor::ConfigEditor;
use crate::components::file_explorer::FileExplorer;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CurrentScreen {
    Home,
    BitcoinConfig,
    P2PoolConfig,
    FileExplorer,
    Exiting,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub sidebar_index: usize,
    pub bitcoin_conf_path: Option<PathBuf>,
    pub explorer: FileExplorer,
    pub explorer_trigger: Option<CurrentScreen>,
    pub p2pool_conf_path: Option<PathBuf>,
    pub p2pool_editor: ConfigEditor,
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::Home,
            sidebar_index: 0,
            bitcoin_conf_path: None,
            explorer: FileExplorer::new(),
            explorer_trigger: None,
            p2pool_conf_path: None,
            p2pool_editor: ConfigEditor::new(),
        }
    }

    pub fn toggle_menu(&mut self) {
        // Logic to switch between sidebar items
        match self.sidebar_index {
            0 => self.current_screen = CurrentScreen::Home,
            1 => self.current_screen = CurrentScreen::BitcoinConfig,
            2 => self.current_screen = CurrentScreen::P2PoolConfig,
            _ => {}
        }
    }
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
