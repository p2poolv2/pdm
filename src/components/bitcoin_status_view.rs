// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction, BITCOIN_STATUS_TABS, BitcoinLogInputMode, ExplorerTrigger};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
};

// Bitcoin Status tabs count
const _: () = assert!(
    BITCOIN_STATUS_TABS.len() == 4,
    "update tab dispatch match in bitcoin_status_view.rs"
);

#[derive(Debug, Clone)]
pub struct BitcoinStatusView;

impl BitcoinStatusView {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    pub fn render(f: &mut Frame, app: &App, area: Rect) {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Tabs bar
                Constraint::Min(0),    // Tab content
            ])
            .split(area);

        let tabs = Tabs::new(BITCOIN_STATUS_TABS.to_vec())
            .block(Block::default().borders(Borders::ALL).title(" Info "))
            .select(app.bitcoin_status_tab)
            .highlight_style(Style::default().bg(Color::Gray).fg(Color::Black));

        f.render_widget(tabs, outer[0]);

        let content_area = outer[1];
        match app.bitcoin_status_tab {
            // Chain Info
            0 => Self::render_chain_info(f, app, content_area),
            // System
            1 => {
                let text = "System";
                let p = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL))
                    .wrap(Wrap { trim: true });
                f.render_widget(p, content_area);
            }
            // Logs
            2 => Self::render_logs(f, app, content_area),
            // Peers
            3 => Self::render_peers(f, app, content_area),
            _ => {}
        }
    }

    pub fn handle_logs_input(app: &mut App, key: KeyEvent) -> AppAction {
        if let Some(mode) = app.bitcoin_log_input_mode {
            return Self::handle_logs_text_input(app, key, mode);
        }

        match key.code {
            KeyCode::Char('/') => {
                app.bitcoin_log_input_mode = Some(BitcoinLogInputMode::Search);
                app.bitcoin_log_input = app.bitcoin_log_filter.clone();
                AppAction::None
            }
            KeyCode::Char('p') => {
                app.bitcoin_log_input_mode = Some(BitcoinLogInputMode::LogFilePath);
                app.bitcoin_log_input = app
                    .bitcoin_log_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                AppAction::None
            }
            KeyCode::Char('g') | KeyCode::Char('d') => {
                app.bitcoin_log_input_mode = Some(BitcoinLogInputMode::DataDirPath);
                app.bitcoin_log_input = app
                    .settings
                    .bitcoin_core_data_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                AppAction::None
            }
            KeyCode::Char('b') => AppAction::OpenExplorer(ExplorerTrigger::BitcoinCoreLogFile),
            KeyCode::Char('o') => AppAction::OpenExplorer(ExplorerTrigger::BitcoinCoreDataDir),
            KeyCode::Char('r') => AppAction::RefreshBitcoinLogs,
            KeyCode::Char('a') => AppAction::ToggleBitcoinLogAutoScroll,
            KeyCode::Char('c') => AppAction::CopyBitcoinLogs,
            KeyCode::Esc if !app.bitcoin_log_filter.is_empty() => {
                app.bitcoin_log_filter.clear();
                app.bitcoin_log_scroll = 0;
                AppAction::None
            }
            KeyCode::Up => {
                app.bitcoin_log_scroll = app.bitcoin_log_scroll.saturating_sub(1);
                app.bitcoin_log_auto_scroll = app.bitcoin_log_scroll == 0;
                AppAction::None
            }
            KeyCode::Down => {
                app.bitcoin_log_scroll = Self::next_scroll(app, 1);
                app.bitcoin_log_auto_scroll = false;
                AppAction::None
            }
            KeyCode::PageUp => {
                app.bitcoin_log_scroll = app.bitcoin_log_scroll.saturating_sub(10);
                app.bitcoin_log_auto_scroll = app.bitcoin_log_scroll == 0;
                AppAction::None
            }
            KeyCode::PageDown => {
                app.bitcoin_log_scroll = Self::next_scroll(app, 10);
                app.bitcoin_log_auto_scroll = false;
                AppAction::None
            }
            KeyCode::Home => {
                app.bitcoin_log_scroll = 0;
                app.bitcoin_log_auto_scroll = true;
                AppAction::None
            }
            KeyCode::End => {
                app.bitcoin_log_scroll = Self::max_scroll(app);
                app.bitcoin_log_auto_scroll = false;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_logs_text_input(
        app: &mut App,
        key: KeyEvent,
        mode: BitcoinLogInputMode,
    ) -> AppAction {
        match key.code {
            KeyCode::Enter => {
                let input = app.bitcoin_log_input.trim().to_string();
                app.bitcoin_log_input.clear();
                app.bitcoin_log_input_mode = None;

                match mode {
                    BitcoinLogInputMode::Search => {
                        app.bitcoin_log_filter = input;
                        app.bitcoin_log_scroll = 0;
                        AppAction::None
                    }
                    BitcoinLogInputMode::LogFilePath if input.is_empty() => {
                        app.bitcoin_log_status = "Log file path cannot be empty.".to_string();
                        AppAction::None
                    }
                    BitcoinLogInputMode::LogFilePath => {
                        AppAction::SetBitcoinLogFile(std::path::PathBuf::from(input))
                    }
                    BitcoinLogInputMode::DataDirPath if input.is_empty() => {
                        app.bitcoin_log_status =
                            "Bitcoin Core data directory cannot be empty.".to_string();
                        AppAction::None
                    }
                    BitcoinLogInputMode::DataDirPath => {
                        AppAction::SetBitcoinLogDataDir(std::path::PathBuf::from(input))
                    }
                }
            }
            KeyCode::Esc => {
                app.bitcoin_log_input.clear();
                app.bitcoin_log_input_mode = None;
                AppAction::None
            }
            KeyCode::Backspace => {
                app.bitcoin_log_input.pop();
                AppAction::None
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                app.bitcoin_log_input.push(ch);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn render_chain_info(f: &mut Frame, app: &App, area: Rect) {
        let text = if app.bitcoin_conf_path.is_none() {
            vec![Line::from(Span::styled(
                "Select a bitcoin.conf file to load Bitcoin Core chain info.",
                Style::default().fg(Color::DarkGray),
            ))]
        } else if let Some(info) = &app.bitcoin_chain_info {
            vec![
                Line::from(format!("Network                : {}", info.network)),
                Line::from(format!("Block Height           : {}", info.block_height)),
                Line::from(format!("Best Block Hash        : {}", info.best_block_hash)),
                Line::from(format!(
                    "Verification Progress  : {}",
                    Self::format_verification_progress(info.verification_progress)
                )),
                Line::from(format!(
                    "Initial Block Download : {}",
                    Self::format_optional_bool(info.initial_block_download)
                )),
                Line::from(format!(
                    "Connection Count       : {}",
                    Self::format_optional_u64(info.connection_count)
                )),
            ]
        } else if let Some(err) = &app.bitcoin_chain_info_error {
            vec![Line::from(Span::styled(
                format!("Failed to fetch Bitcoin chain info: {err}"),
                Style::default().fg(Color::Red),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Loading Bitcoin chain info...",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Chain Info "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn render_peers(f: &mut Frame, app: &App, area: Rect) {
        let text = if app.bitcoin_conf_path.is_none() {
            vec![Line::from(Span::styled(
                "Select a bitcoin.conf file to load Bitcoin Core peer info.",
                Style::default().fg(Color::DarkGray),
            ))]
        } else if let Some(info) = &app.bitcoin_chain_info {
            let mut lines = Vec::with_capacity(info.connected_peer_addresses.len() + 3);
            lines.push(Line::from(format!(
                "Connected Peers: {}",
                info.connected_peer_addresses.len()
            )));
            lines.push(Line::from(""));
            lines.push(Line::from("Peer Addresses:"));

            if info.connected_peer_addresses.is_empty() {
                lines.push(Line::from("None"));
            } else {
                lines.extend(
                    info.connected_peer_addresses
                        .iter()
                        .map(|address| Line::from(format!("* {address}"))),
                );
            }

            lines
        } else if let Some(err) = &app.bitcoin_chain_info_error {
            vec![Line::from(Span::styled(
                format!("Failed to fetch Bitcoin peer info: {err}"),
                Style::default().fg(Color::Red),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Loading Bitcoin peer info...",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Peers "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn render_logs(f: &mut Frame, app: &App, area: Rect) {
        let constraints = if app.bitcoin_log_input_mode.is_some() {
            vec![
                Constraint::Length(6),
                Constraint::Length(3),
                Constraint::Min(0),
            ]
        } else {
            vec![Constraint::Length(6), Constraint::Min(0)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let path = app
            .bitcoin_log_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(not configured)".to_string());
        let filtered_count = app.filtered_bitcoin_log_lines().len();
        let total_count = app.bitcoin_log_lines.len();
        let filter = if app.bitcoin_log_filter.trim().is_empty() {
            "(none)".to_string()
        } else {
            app.bitcoin_log_filter.clone()
        };
        let auto_scroll = if app.bitcoin_log_auto_scroll {
            "on"
        } else {
            "off"
        };

        let summary = vec![
            Line::from(vec![
                Span::styled("Current log file path: ", Style::default().fg(Color::Gray)),
                Span::raw(path),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Gray)),
                Span::raw(app.bitcoin_log_status.clone()),
            ]),
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(Color::Gray)),
                Span::raw(filter),
                Span::styled("  Auto-scroll: ", Style::default().fg(Color::Gray)),
                Span::raw(auto_scroll),
                Span::styled("  Lines: ", Style::default().fg(Color::Gray)),
                Span::raw(format!("{filtered_count}/{total_count}")),
            ]),
            Self::log_controls_line(),
        ];

        let summary_panel = Paragraph::new(summary)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Bitcoin Core Logs "),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(summary_panel, chunks[0]);

        let log_area = if app.bitcoin_log_input_mode.is_some() {
            let input_panel = Paragraph::new(app.bitcoin_log_input.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(Self::input_title(app.bitcoin_log_input_mode)),
                )
                .style(Style::default().fg(Color::White));
            f.render_widget(input_panel, chunks[1]);
            chunks[2]
        } else {
            chunks[1]
        };

        let log_lines = Self::log_lines(app);
        let log_panel = Paragraph::new(log_lines)
            .block(Block::default().borders(Borders::ALL).title(" debug.log "))
            .style(Style::default().bg(Color::Black).fg(Color::LightGreen))
            .scroll((app.bitcoin_log_scroll, 0));
        f.render_widget(log_panel, log_area);
    }

    fn log_controls_line() -> Line<'static> {
        Line::from(vec![
            Span::styled("[b] Browse log ", Style::default().fg(Color::Cyan)),
            Span::styled("[o] Data dir ", Style::default().fg(Color::Cyan)),
            Span::styled("[p] Path ", Style::default().fg(Color::Cyan)),
            Span::styled("[g] Dir path ", Style::default().fg(Color::Cyan)),
            Span::styled("[/] Search ", Style::default().fg(Color::Cyan)),
            Span::styled("[r] Refresh ", Style::default().fg(Color::Cyan)),
            Span::styled("[a] Auto ", Style::default().fg(Color::Cyan)),
            Span::styled("[c] Copy", Style::default().fg(Color::Cyan)),
        ])
    }

    fn input_title(mode: Option<BitcoinLogInputMode>) -> &'static str {
        match mode {
            Some(BitcoinLogInputMode::Search) => " Search/filter logs ",
            Some(BitcoinLogInputMode::LogFilePath) => " Bitcoin Core debug.log path ",
            Some(BitcoinLogInputMode::DataDirPath) => " Bitcoin Core data directory ",
            None => " Input ",
        }
    }

    fn log_lines(app: &App) -> Vec<Line<'static>> {
        if app.bitcoin_log_path.is_none() {
            return vec![Line::from(Span::styled(
                "No Bitcoin Core debug.log found. Choose a log file or Bitcoin data directory.",
                Style::default().fg(Color::Yellow),
            ))];
        }

        if app.bitcoin_log_lines.is_empty() {
            return vec![Line::from(Span::styled(
                app.bitcoin_log_status.clone(),
                Style::default().fg(Color::DarkGray),
            ))];
        }

        let filtered = app.filtered_bitcoin_log_lines();
        if filtered.is_empty() {
            return vec![Line::from(Span::styled(
                "No log entries match the current filter.",
                Style::default().fg(Color::DarkGray),
            ))];
        }

        filtered
            .into_iter()
            .map(|line| Line::from(Span::raw(line.to_string())))
            .collect()
    }

    fn max_scroll(app: &App) -> u16 {
        app.filtered_bitcoin_log_lines()
            .len()
            .saturating_sub(1)
            .min(u16::MAX as usize) as u16
    }

    fn next_scroll(app: &App, delta: u16) -> u16 {
        app.bitcoin_log_scroll
            .saturating_add(delta)
            .min(Self::max_scroll(app))
    }

    fn format_verification_progress(progress: Option<f64>) -> String {
        progress.map_or_else(|| "-".to_string(), |value| format!("{:.2}%", value * 100.0))
    }

    fn format_optional_bool(value: Option<bool>) -> &'static str {
        match value {
            Some(true) => "yes",
            Some(false) => "no",
            None => "-",
        }
    }

    fn format_optional_u64(value: Option<u64>) -> String {
        value.map_or_else(|| "-".to_string(), |value| value.to_string())
    }
}

impl Default for BitcoinStatusView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::components::bitcoin_client::BitcoinChainInfo;
    use ratatui::{Terminal, backend::TestBackend, prelude::Rect};
    use std::path::PathBuf;

    fn render_view(app: &App) -> String {
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 25);

        terminal
            .draw(|f| BitcoinStatusView::render(f, app, area))
            .unwrap();

        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_prompt_when_no_bitcoin_conf_is_selected() {
        let app = App::new();

        let output = render_view(&app);

        assert!(output.contains("Select a bitcoin.conf file to load Bitcoin Core chain info."));
        assert!(!output.contains("Loading Bitcoin chain info"));
        assert!(!output.contains("Failed to fetch Bitcoin chain info"));
    }

    #[test]
    fn renders_loaded_chain_info_with_formatted_values() {
        let mut app = App::new();
        app.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_chain_info = Some(BitcoinChainInfo {
            network: "mainnet".to_string(),
            block_height: 850_000,
            best_block_hash: "abc123".to_string(),
            verification_progress: Some(0.9123),
            initial_block_download: Some(true),
            connection_count: Some(7),
            connected_peer_addresses: vec![
                "192.168.1.100:8333".to_string(),
                "192.168.1.101:8333".to_string(),
            ],
        });

        let output = render_view(&app).replace("                ", " ");

        assert!(output.contains("Network : mainnet"));
        assert!(output.contains("Block Height           : 850000"));
        assert!(output.contains("Best Block Hash        : abc123"));
        assert!(output.contains("Verification Progress  : 91.23%"));
        assert!(output.contains("Initial Block Download : yes"));
        assert!(output.contains("Connection Count       : 7"));
        assert!(!output.contains("Loading Bitcoin chain info"));
        assert!(!output.contains("Failed to fetch Bitcoin chain info"));
    }

    #[test]
    fn renders_loading_state_when_chain_info_is_pending() {
        let mut app = App::new();
        app.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));

        let output = render_view(&app);

        assert!(output.contains("Loading Bitcoin chain info..."));
        assert!(!output.contains("Select a bitcoin.conf file"));
        assert!(!output.contains("Failed to fetch Bitcoin chain info"));
    }

    #[test]
    fn renders_error_state_when_chain_info_fetch_fails() {
        let mut app = App::new();
        app.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_chain_info_error = Some("RPC offline".to_string());

        let output = render_view(&app);

        assert!(output.contains("Failed to fetch Bitcoin chain info: RPC offline"));
        assert!(!output.contains("Loading Bitcoin chain info"));
        assert!(!output.contains("Network"));
    }

    #[test]
    fn renders_none_and_false_formatting_for_optional_values() {
        let mut app = App::new();
        app.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_chain_info = Some(BitcoinChainInfo {
            network: "testnet".to_string(),
            block_height: 42,
            best_block_hash: "def456".to_string(),
            verification_progress: None,
            initial_block_download: Some(false),
            connection_count: None,
            connected_peer_addresses: vec![],
        });

        let output = render_view(&app);

        assert!(output.contains("Verification Progress  : -"));
        assert!(output.contains("Initial Block Download : no"));
        assert!(output.contains("Connection Count       : -"));
    }
}
