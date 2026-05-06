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
                    info.chain_tip_height.unwrap_or(0)
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
