// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub enum CurrentScreen {
    Home,
    BitcoinConfig,
    FileExplorer,
    Exiting,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub sidebar_index: usize,
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::Home,
            sidebar_index: 0,
        }
    }

    pub fn toggle_menu(&mut self) {
        // Logic to switch between sidebar items
        match self.sidebar_index {
            0 => self.current_screen = CurrentScreen::Home,
            1 => self.current_screen = CurrentScreen::BitcoinConfig,
            _ => {}
        }
    }
}
