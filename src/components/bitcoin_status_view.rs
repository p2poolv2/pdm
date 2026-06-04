// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, BITCOIN_STATUS_TABS};

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
            2 => {
                let text = "Logs";
                let p = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL))
                    .wrap(Wrap { trim: true });
                f.render_widget(p, content_area);
            }
            // Peers
            3 => {
                let text = "Peers";
                let p = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL))
                    .wrap(Wrap { trim: true });
                f.render_widget(p, content_area);
            }
            _ => {}
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
