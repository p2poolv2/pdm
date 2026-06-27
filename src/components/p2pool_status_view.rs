// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, P2POOL_STATUS_TABS};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
};

#[derive(Debug, Clone)]
pub struct P2PoolStatusView;

impl P2PoolStatusView {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    // P2Pool Status
    pub fn render(f: &mut Frame, app: &App, area: Rect) {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Tabs bar
                Constraint::Min(0),    // Content area
            ])
            .split(area);

        let tabs = Tabs::new(P2POOL_STATUS_TABS.to_vec())
            .block(Block::default().borders(Borders::ALL).title(" Info "))
            .select(app.p2pool_status_tab)
            .highlight_style(Style::default().bg(Color::Gray).fg(Color::Black));

        f.render_widget(tabs, outer[0]);

        match app.p2pool_status_tab {
            0 => Self::render_chain_info(f, app, outer[1]),
            1 => Self::render_peer_info(f, app, outer[1]),
            _ => {}
        }
    }

    fn render_chain_info(f: &mut Frame, app: &App, area: Rect) {
        let text = if let Some(info) = &app.chain_info {
            vec![
                Line::from(format!(
                    "Genesis Blockhash      : {}",
                    info.genesis_blockhash.as_deref().unwrap_or("-")
                )),
                Line::from(format!(
                    "Chain Tip Height       : {}",
                    info.chain_tip_height
                        .map_or_else(|| "-".to_string(), |h| h.to_string())
                )),
                Line::from(format!(
                    "Chain Tip Blockhash    : {}",
                    info.chain_tip_blockhash.as_deref().unwrap_or("-")
                )),
                Line::from(format!("Total Work             : {}", info.total_work)),
            ]
        } else if let Some(err) = &app.p2pool_chain_info_error {
            vec![Line::from(Span::styled(
                format!("Failed to fetch chain info: {err}"),
                Style::default().fg(Color::Red),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Loading chain info...",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Chain Info "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn render_peer_info(f: &mut Frame, app: &App, area: Rect) {
        let text = if let Some(peers) = &app.peer_info {
            if peers.is_empty() {
                vec![Line::from(Span::styled(
                    "No connected peers",
                    Style::default().fg(Color::DarkGray),
                ))]
            } else {
                let mut lines = Vec::with_capacity(peers.len() + 2);
                lines.push(Line::from(format!(
                    "Connected Peers        : {}",
                    peers.len()
                )));
                lines.push(Line::from(""));

                for peer in peers {
                    lines.push(Line::from(format!(
                        "{} ({})",
                        peer.peer_id,
                        peer.status.as_deref().unwrap_or("Connected")
                    )));
                }

                lines
            }
        } else if let Some(err) = &app.p2pool_peer_info_error {
            vec![Line::from(Span::styled(
                format!("Failed to fetch peer info: {err}"),
                Style::default().fg(Color::Red),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Loading peer info...",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Peers Info "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }
}

impl Default for P2PoolStatusView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::p2pool_client::{ChainInfo, PeerInfo};
    use ratatui::{Terminal, backend::TestBackend, prelude::Rect};

    fn render_view(app: &App) -> String {
        let backend = TestBackend::new(100, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 100, 25);

        terminal
            .draw(|f| P2PoolStatusView::render(f, app, area))
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
    fn render_dispatches_chain_info_for_tab_zero() {
        let app = App::new();

        let output = render_view(&app);

        assert!(output.contains("Loading chain info..."));
    }

    #[test]
    fn render_dispatches_peer_info_for_tab_one() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;

        let output = render_view(&app);

        assert!(output.contains("Loading peer info..."));
    }

    #[test]
    fn render_with_unknown_tab_only_renders_tabs() {
        let mut app = App::new();
        app.p2pool_status_tab = 5;

        let output = render_view(&app);

        assert!(output.contains("Info"));
        assert!(!output.contains("Loading chain info..."));
        assert!(!output.contains("Genesis Blockhash"));
        assert!(!output.contains("Loading peer info..."));
        assert!(!output.contains("No connected peers"));
        assert!(!output.contains("Connected Peers"));
    }

    #[test]
    fn render_chain_info_shows_available_values() {
        let mut app = App::new();
        app.chain_info = Some(ChainInfo {
            genesis_blockhash: Some("genesis-hash".to_string()),
            chain_tip_height: Some(850_000),
            chain_tip_blockhash: Some("tip-hash".to_string()),
            total_work: "ffff".to_string(),
        });

        let output = render_view(&app);

        assert!(output.contains("Genesis Blockhash      : genesis-hash"));
        assert!(output.contains("Chain Tip Height       : 850000"));
        assert!(output.contains("Chain Tip Blockhash    : tip-hash"));
        assert!(output.contains("Total Work             : ffff"));
    }

    #[test]
    fn render_chain_info_shows_dash_for_missing_optional_values() {
        let mut app = App::new();
        app.chain_info = Some(ChainInfo {
            genesis_blockhash: None,
            chain_tip_height: None,
            chain_tip_blockhash: None,
            total_work: "0".to_string(),
        });

        let output = render_view(&app);

        assert!(output.contains("Genesis Blockhash      : -"));
        assert!(output.contains("Chain Tip Height       : -"));
        assert!(output.contains("Chain Tip Blockhash    : -"));
        assert!(output.contains("Total Work             : 0"));
    }

    #[test]
    fn render_chain_info_shows_error_when_fetch_failed() {
        let mut app = App::new();
        app.p2pool_chain_info_error = Some("connection refused".to_string());

        let output = render_view(&app);

        assert!(output.contains("Failed to fetch chain info: connection refused"));
        assert!(!output.contains("Loading chain info..."));
    }

    #[test]
    fn render_peer_info_shows_loading_state_with_no_data() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;

        let output = render_view(&app);

        assert!(output.contains("Loading peer info..."));
    }

    #[test]
    fn render_peer_info_shows_error_when_fetch_failed() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;
        app.p2pool_peer_info_error = Some("request timed out".to_string());

        let output = render_view(&app);

        assert!(output.contains("Failed to fetch peer info: request timed out"));
        assert!(!output.contains("Loading peer info..."));
    }

    #[test]
    fn render_peer_info_shows_empty_state_when_no_peers_are_connected() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;
        app.peer_info = Some(Vec::new());

        let output = render_view(&app);

        assert!(output.contains("No connected peers"));
    }

    #[test]
    fn render_peer_info_lists_connected_peers_with_statuses() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;
        app.peer_info = Some(vec![
            PeerInfo {
                peer_id: "12D3KooWPeerOne".to_string(),
                status: Some("Connected".to_string()),
            },
            PeerInfo {
                peer_id: "12D3KooWPeerTwo".to_string(),
                status: Some("Syncing".to_string()),
            },
        ]);

        let output = render_view(&app);

        assert!(output.contains("Connected Peers        : 2"));
        assert!(output.contains("12D3KooWPeerOne (Connected)"));
        assert!(output.contains("12D3KooWPeerTwo (Syncing)"));
    }

    #[test]
    fn render_peer_info_defaults_missing_status_to_connected() {
        let mut app = App::new();
        app.p2pool_status_tab = 1;
        app.peer_info = Some(vec![PeerInfo {
            peer_id: "12D3KooWNoStatus".to_string(),
            status: None,
        }]);

        let output = render_view(&app);

        assert!(output.contains("Connected Peers        : 1"));
        assert!(output.contains("12D3KooWNoStatus (Connected)"));
    }
}
