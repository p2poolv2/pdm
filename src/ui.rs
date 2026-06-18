// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app;
use crate::app::{App, CurrentScreen};
use crate::components::{
    bitcoin_config_view::BitcoinConfigView, bitcoin_status_view::BitcoinStatusView,
    file_explorer::FileExplorer, home_view::HomeView, ln_config_view::LNConfigView,
    ln_status_view::LNStatusView, p2pool_config_view::P2PoolConfigView,
    p2pool_status_view::P2PoolStatusView, settings_view::SettingsView,
    shares_market_view::SharesMarketView, status_bar::StatusBar,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState},
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    let main_row = outer[0];
    let status_bar_area = outer[1];

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Sidebar
            Constraint::Min(0),     // Main Content
        ])
        .split(main_row);

    //  Sidebar
    let items: Vec<ListItem> = app::SIDEBAR_ITEMS
        .iter()
        .map(|&(label, _)| ListItem::new(label))
        .collect();

    // Highlight the active one
    let mut state = ListState::default();
    state.select(Some(app.sidebar_index));

    // Dim the sidebar when the user has moved focus into a content panel
    let sidebar_focused = match app.current_screen {
        CurrentScreen::BitcoinConfig => app.bitcoin_config_view.sidebar_focused,
        CurrentScreen::Settings => app.settings_view.sidebar_focused,
        _ => true,
    };
    let sidebar_border_style = if sidebar_focused {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let sidebar = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" PDM ")
                .border_style(sidebar_border_style),
        )
        .highlight_style(Style::default().bg(Color::Gray).fg(Color::Black));

    f.render_stateful_widget(sidebar, chunks[0], &mut state);

    // Main Content
    let main_area = chunks[1];

    match app.current_screen {
        CurrentScreen::Home => {
            HomeView::render(f, app, main_area);
        }
        CurrentScreen::BitcoinConfig => {
            BitcoinConfigView::render(f, app, main_area);
        }
        CurrentScreen::BitcoinStatus => {
            BitcoinStatusView::render(f, app, main_area);
        }
        CurrentScreen::P2PoolConfig => {
            P2PoolConfigView::render(f, app, main_area);
        }
        CurrentScreen::P2PoolStatus => {
            P2PoolStatusView::render(f, app, main_area);
        }
        CurrentScreen::LNConfig => {
            LNConfigView::render(f, app, main_area);
        }
        CurrentScreen::LNStatus => {
            LNStatusView::render(f, app, main_area);
        }
        CurrentScreen::SharesMarket => {
            SharesMarketView::render(f, app, main_area);
        }
        CurrentScreen::FileExplorer => {
            FileExplorer::render(f, app, main_area);
        }
        CurrentScreen::Settings => {
            SettingsView::render(f, app, main_area);
        }
    }

    StatusBar::render(f, app, status_bar_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::components::p2pool_client::P2PoolClient;
    use mockito::Server;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    #[test]
    fn test_home_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_bitcoin_config_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 1;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_bitcoin_status_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 2;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_bitcoin_status_tab_system_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 2;
        app.toggle_menu();
        app.bitcoin_status_tab = 1;
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_bitcoin_status_tab_logs_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 2;
        app.toggle_menu();
        app.bitcoin_status_tab = 2;
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_bitcoin_status_tab_peers_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 2;
        app.toggle_menu();
        app.bitcoin_status_tab = 3;
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_p2pool_config_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 3;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_p2pool_status_screen_render() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/chain_info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "genesis_blockhash": null,
                    "chain_tip_height": 1,
                    "total_work": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "chain_tip_blockhash": null
                })
                .to_string(),
            )
            .create();

        let client = P2PoolClient::with_base_url(server.url());
        let mut app = App::new_with_client(client);
        let mut terminal = make_terminal();
        app.sidebar_index = 4;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_ln_config_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 5;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_ln_status_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 6;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    fn test_shares_market_screen_render() {
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 7;
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!(terminal.backend());
    }

    #[test]
    #[serial_test::serial]
    fn test_settings_screen_render() {
        // Fix PDM_CONFIG_DIR so field 4 renders a deterministic path across platforms.
        // SAFETY: serialised by #[serial] — no concurrent mutation of PDM_CONFIG_DIR.
        unsafe { std::env::set_var("PDM_CONFIG_DIR", "/pdm/test-config") };
        let mut terminal = make_terminal();
        let mut app = App::new();
        app.sidebar_index = 8; // Settings
        app.toggle_menu();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        unsafe { std::env::remove_var("PDM_CONFIG_DIR") };
        insta::assert_debug_snapshot!(terminal.backend());
    }
}
