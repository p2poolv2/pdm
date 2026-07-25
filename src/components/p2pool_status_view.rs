// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction, P2POOL_STATUS_TABS};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs, Wrap},
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct P2PoolStatusView;

#[derive(Debug)]
struct ShareTableEntry {
    height: u64,
    blockhash: String,
    miner: String,
    bits: String,
    timestamp: u64,
    uncles: usize,
}

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
            1 => Self::render_share_info(f, app, outer[1]),
            2 => Self::render_peer_info(f, app, outer[1]),
            3 => Self::render_process(f, app, outer[1]),
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

    fn render_share_info(f: &mut Frame, app: &App, area: Rect) {
        let mut rows = Self::share_rows(app);
        if rows.is_empty() {
            rows.push(Self::message_row(Self::share_empty_message(app)));
        }

        let header = Row::new([
            "Height",
            "Blockhash",
            "Miner",
            "Difficulty",
            "Time",
            "Uncles",
        ])
        .style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);
        let widths = [
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(12),
            Constraint::Length(6),
        ];

        let table = Table::new(rows, widths)
            .block(Block::default().borders(Borders::ALL).title(" Shares "))
            .header(header)
            .column_spacing(1)
            .style(Style::default().fg(Color::White));

        f.render_widget(table, area);
    }

    fn render_peer_info(f: &mut Frame, app: &App, area: Rect) {
        let mut text = if let Some(peers) = &app.peer_info {
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

        if let Some(err) = &app.p2pool_live_error {
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                format!("Live stream error: {err}"),
                Style::default().fg(Color::Red),
            )));
        }

        if !app.live_peer_events.is_empty() {
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                "Live Peer Events",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for event in app.live_peer_events.iter().rev().take(8) {
                text.push(Line::from(format!(
                    "{}: {}",
                    event.status,
                    Self::short_value(&event.peer_id, 42)
                )));
            }
        }

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Peers Info "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    pub fn handle_process_input(app: &mut App, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('s') if app.p2pool_process_state.can_start() => AppAction::StartP2Pool,
            KeyCode::Char('t') if app.p2pool_process_state.can_stop() => AppAction::StopP2Pool,
            KeyCode::Char('r') if app.p2pool_process_state.can_restart() => {
                AppAction::RestartP2Pool
            }
            _ => AppAction::None,
        }
    }

    fn render_process(f: &mut Frame, app: &App, area: Rect) {
        let state = app.p2pool_process_state.as_str();
        let error = app.p2pool_process_error.clone().unwrap_or_default();
        let status_line = if error.is_empty() {
            format!("Process state: {state}")
        } else {
            format!("Process state: {state} ({error})")
        };
        let start_style = if app.p2pool_process_state.can_start() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let stop_style = if app.p2pool_process_state.can_stop() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let restart_style = if app.p2pool_process_state.can_restart() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let controls = vec![
            Line::from(vec![
                Span::styled("[s] Start ", start_style),
                Span::styled("[t] Stop ", stop_style),
                Span::styled("[r] Restart", restart_style),
            ]),
            Line::from(""),
            Line::from(status_line),
        ];
        let paragraph = Paragraph::new(controls)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" P2Poolv2 Process "),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn short_value(value: &str, max_len: usize) -> String {
        if value.len() <= max_len {
            return value.to_string();
        }

        if max_len <= 3 {
            return value.chars().take(max_len).collect();
        }

        let head_len = (max_len - 3) / 2;
        let tail_len = max_len - 3 - head_len;
        let head: String = value.chars().take(head_len).collect();
        let tail: String = value
            .chars()
            .rev()
            .take(tail_len)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{head}...{tail}")
    }

    fn share_rows(app: &App) -> Vec<Row<'static>> {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        for share in app.live_shares.iter().rev() {
            if seen.insert(share.blockhash.clone()) {
                entries.push(ShareTableEntry {
                    height: share.height,
                    blockhash: share.blockhash.clone(),
                    miner: share.miner_address.clone(),
                    bits: share.bits.clone(),
                    timestamp: share.timestamp,
                    uncles: share.uncles.len(),
                });
            }
        }

        if let Some(info) = &app.share_info {
            for share in info.shares.iter().rev() {
                if seen.insert(share.blockhash.clone()) {
                    entries.push(ShareTableEntry {
                        height: share.height,
                        blockhash: share.blockhash.clone(),
                        miner: share.miner_address.clone(),
                        bits: share.bits.clone(),
                        timestamp: share.timestamp,
                        uncles: share.uncles.len(),
                    });
                }
            }
        }

        entries.sort_by(|left, right| {
            right
                .height
                .cmp(&left.height)
                .then_with(|| right.timestamp.cmp(&left.timestamp))
        });

        entries
            .into_iter()
            .take(50)
            .map(|entry| {
                Self::share_row(
                    entry.height,
                    &entry.blockhash,
                    &entry.miner,
                    &entry.bits,
                    entry.timestamp,
                    entry.uncles,
                )
            })
            .collect()
    }

    fn share_row(
        height: u64,
        blockhash: &str,
        miner: &str,
        bits: &str,
        timestamp: u64,
        uncles: usize,
    ) -> Row<'static> {
        Row::new(vec![
            Cell::from(height.to_string()),
            Self::chip(Self::short_value(blockhash, 10)),
            Self::chip(Self::short_value(miner, 10)),
            Cell::from(Self::format_difficulty(bits)),
            Cell::from(Self::format_timestamp(timestamp)),
            Cell::from(uncles.to_string()),
        ])
        .height(1)
    }

    fn message_row(message: String) -> Row<'static> {
        Row::new(vec![
            Cell::from(Span::styled(message, Style::default().fg(Color::DarkGray))),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])
    }

    fn share_empty_message(app: &App) -> String {
        if let Some(err) = &app.p2pool_share_info_error {
            return format!("Recent shares unavailable: {}", Self::short_value(err, 64));
        }

        if let Some(err) = &app.p2pool_live_error {
            return format!("Live shares unavailable: {}", Self::short_value(err, 64));
        }

        "Waiting for share data...".to_string()
    }

    fn chip(value: String) -> Cell<'static> {
        Cell::from(Span::styled(
            value,
            Style::default().fg(Color::Gray).bg(Color::Black),
        ))
    }

    fn format_difficulty(bits: &str) -> String {
        let Some(bits) = Self::parse_bits(bits) else {
            return Self::short_value(bits, 10);
        };

        let exponent = (bits >> 24) as i32;
        let mantissa = bits & 0x00ff_ffff;
        if mantissa == 0 {
            return "-".to_string();
        }

        let difficulty = (0x0000_ffff_u32 as f64 / mantissa as f64) * 256_f64.powi(0x1d - exponent);
        if !difficulty.is_finite() || difficulty <= 0.0 {
            return "-".to_string();
        }

        if difficulty >= 100.0 {
            return Self::format_integer_with_commas(difficulty.round() as u64);
        }

        if difficulty >= 1.0 {
            return format!("{difficulty:.2}");
        }

        format!("{difficulty:.4}")
    }

    fn parse_bits(bits: &str) -> Option<u32> {
        let value = bits.trim();
        if value.is_empty() {
            return None;
        }

        if let Some(hex) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        {
            return u32::from_str_radix(hex, 16).ok();
        }

        if value
            .chars()
            .any(|c| c.is_ascii_hexdigit() && c.is_ascii_alphabetic())
        {
            return u32::from_str_radix(value, 16).ok();
        }

        value.parse::<u32>().ok()
    }

    fn format_integer_with_commas(value: u64) -> String {
        let digits = value.to_string();
        let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
        for (index, digit) in digits.chars().rev().enumerate() {
            if index > 0 && index % 3 == 0 {
                formatted.push(',');
            }
            formatted.push(digit);
        }
        formatted.chars().rev().collect()
    }

    fn format_timestamp(timestamp: u64) -> String {
        let timestamp = if timestamp > 10_000_000_000 {
            timestamp / 1_000
        } else {
            timestamp
        };
        let days = (timestamp / 86_400) as i64;
        let seconds = timestamp % 86_400;
        let hour = seconds / 3_600;
        let minute = (seconds % 3_600) / 60;
        let second = seconds % 60;
        let (year, month, day) = Self::civil_from_days(days);
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let hour = match hour % 12 {
            0 => 12,
            value => value,
        };

        format!("{month}/{day}/{year}, {hour}:{minute:02}:{second:02} {suffix}")
    }

    fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
        let days = days_since_epoch + 719_468;
        let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
        let day_of_era = days - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_param = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_param + 2) / 5 + 1;
        let month = month_param + if month_param < 10 { 3 } else { -9 };
        let year = year + if month <= 2 { 1 } else { 0 };

        (year as i32, month as u32, day as u32)
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
    use crate::components::p2pool_client::{
        ChainInfo, PeerInfo, ShareInfo, SharesResponse, UncleInfo,
    };
    use crate::components::p2pool_websocket::{LivePeerEvent, LiveShare};
    use ratatui::{Terminal, backend::TestBackend, prelude::Rect};

    const SHARE_TAB: usize = 1;
    const PEER_TAB: usize = 2;

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

    fn share_info(
        height: u64,
        blockhash: &str,
        miner_address: &str,
        timestamp: u64,
        bits: &str,
        uncle_count: usize,
    ) -> ShareInfo {
        ShareInfo {
            blockhash: blockhash.to_string(),
            prev_blockhash: format!("{blockhash}-prev"),
            height,
            miner_address: miner_address.to_string(),
            timestamp,
            bits: bits.to_string(),
            uncles: (0..uncle_count)
                .map(|index| UncleInfo {
                    blockhash: format!("{blockhash}-uncle-{index}"),
                    prev_blockhash: format!("{blockhash}-uncle-prev-{index}"),
                    miner_address: miner_address.to_string(),
                    timestamp,
                    height,
                })
                .collect(),
        }
    }

    fn live_share(
        height: u64,
        blockhash: &str,
        miner_address: &str,
        timestamp: u64,
        bits: &str,
        uncle_count: usize,
    ) -> LiveShare {
        LiveShare {
            blockhash: blockhash.to_string(),
            prev_blockhash: format!("{blockhash}-prev"),
            height,
            miner_address: miner_address.to_string(),
            timestamp,
            bits: bits.to_string(),
            uncles: (0..uncle_count)
                .map(|index| format!("{blockhash}-uncle-{index}"))
                .collect(),
        }
    }

    #[test]
    fn default_constructs_status_view() {
        let view = P2PoolStatusView;

        assert_eq!(format!("{view:?}"), "P2PoolStatusView");
    }

    #[test]
    fn render_dispatches_chain_info_for_tab_zero() {
        let app = App::new();

        let output = render_view(&app);

        assert!(output.contains("Loading chain info..."));
    }

    #[test]
    fn render_dispatches_share_info_for_tab_one() {
        let mut app = App::new();
        app.p2pool_status_tab = SHARE_TAB;

        let output = render_view(&app);

        assert!(output.contains("Difficulty"));
        assert!(output.contains("Uncles"));
    }

    #[test]
    fn render_share_info_lists_api_and_live_shares_without_duplicates() {
        let mut app = App::new();
        app.p2pool_status_tab = SHARE_TAB;
        app.live_shares = vec![live_share(
            42,
            "livehash",
            "olderlive",
            1_700_000_000,
            "1d00ffff",
            1,
        )];
        app.share_info = Some(SharesResponse {
            from_height: 40,
            to_height: 42,
            shares: vec![
                share_info(42, "apihash", "newerapi", 1_700_000_060, "1e00ffff", 2),
                share_info(41, "livehash", "dupeapi", 1_700_000_120, "1d00ffff", 0),
            ],
        });

        let output = render_view(&app);

        assert!(output.contains("newerapi"));
        assert!(output.contains("olderlive"));
        assert!(!output.contains("dupeapi"));
        assert!(output.contains("0.0039"));
        assert!(output.contains("1.00"));
        assert!(output.contains("11/14/2023, 10:14:20 PM"));
        assert!(output.find("newerapi").unwrap() < output.find("olderlive").unwrap());
    }

    #[test]
    fn render_share_info_shows_recent_share_error_before_live_error() {
        let mut app = App::new();
        app.p2pool_status_tab = SHARE_TAB;
        app.p2pool_share_info_error = Some("recent fetch failed".to_string());
        app.p2pool_live_error = Some("websocket closed".to_string());

        let output = render_view(&app);

        assert!(output.contains("Recent"));
        assert!(!output.contains("Live sh"));
        assert!(!output.contains("Waiting"));
    }

    #[test]
    fn render_share_info_shows_live_error_when_recent_fetch_has_not_failed() {
        let mut app = App::new();
        app.p2pool_status_tab = SHARE_TAB;
        app.p2pool_live_error = Some("websocket closed".to_string());

        let output = render_view(&app);

        assert!(output.contains("Live sh"));
        assert!(!output.contains("Waiting"));
    }

    #[test]
    fn render_dispatches_peer_info_for_tab_two() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;

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
        app.p2pool_status_tab = PEER_TAB;

        let output = render_view(&app);

        assert!(output.contains("Loading peer info..."));
    }

    #[test]
    fn render_peer_info_shows_error_when_fetch_failed() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;
        app.p2pool_peer_info_error = Some("request timed out".to_string());

        let output = render_view(&app);

        assert!(output.contains("Failed to fetch peer info: request timed out"));
        assert!(!output.contains("Loading peer info..."));
    }

    #[test]
    fn render_peer_info_shows_empty_state_when_no_peers_are_connected() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;
        app.peer_info = Some(Vec::new());

        let output = render_view(&app);

        assert!(output.contains("No connected peers"));
    }

    #[test]
    fn render_peer_info_lists_connected_peers_with_statuses() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;
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
    fn render_peer_info_appends_live_error_and_recent_peer_events() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;
        app.peer_info = Some(Vec::new());
        app.p2pool_live_error = Some("websocket closed".to_string());
        app.live_peer_events = (1..=9)
            .map(|index| LivePeerEvent {
                peer_id: format!("12D3KooWPeerEvent{index}"),
                status: format!("Status{index}"),
            })
            .collect();

        let output = render_view(&app);

        assert!(output.contains("No connected peers"));
        assert!(output.contains("Live stream error: websocket closed"));
        assert!(output.contains("Live Peer Events"));
        assert!(output.contains("Status9: 12D3KooWPeerEvent9"));
        assert!(output.contains("Status2: 12D3KooWPeerEvent2"));
        assert!(!output.contains("Status1: 12D3KooWPeerEvent1"));
        assert!(output.find("Status9").unwrap() < output.find("Status2").unwrap());
    }

    #[test]
    fn render_peer_info_defaults_missing_status_to_connected() {
        let mut app = App::new();
        app.p2pool_status_tab = PEER_TAB;
        app.peer_info = Some(vec![PeerInfo {
            peer_id: "12D3KooWNoStatus".to_string(),
            status: None,
        }]);

        let output = render_view(&app);

        assert!(output.contains("Connected Peers        : 1"));
        assert!(output.contains("12D3KooWNoStatus (Connected)"));
    }

    #[test]
    fn render_share_info_deduplicates_duplicate_live_shares() {
        let mut app = App::new();
        app.p2pool_status_tab = SHARE_TAB;

        app.live_shares = vec![
            live_share(42, "samehash", "duplicated", 1_700_000_000, "1d00ffff", 0),
            live_share(42, "samehash", "duplicated", 1_700_000_001, "1d00ffff", 0),
        ];

        let output = render_view(&app);

        assert_eq!(output.matches("duplicated").count(), 1);
    }

    #[test]
    fn short_value_preserves_short_values_and_truncates_long_values() {
        assert_eq!(P2PoolStatusView::short_value("short", 10), "short");
        assert_eq!(
            P2PoolStatusView::short_value("abcdefghijklmnop", 10),
            "abc...mnop"
        );
        assert_eq!(P2PoolStatusView::short_value("abcdef", 3), "abc");
    }

    #[test]
    fn format_difficulty_formats_valid_bits_values() {
        assert_eq!(P2PoolStatusView::format_difficulty("1d00ffff"), "1.00");
        assert_eq!(P2PoolStatusView::format_difficulty("486604799"), "1.00");
        assert_eq!(P2PoolStatusView::format_difficulty("0X1D00FFFF"), "1.00");
        assert_eq!(P2PoolStatusView::format_difficulty("1b00ffff"), "65,536");
        assert_eq!(P2PoolStatusView::format_difficulty("1e00ffff"), "0.0039");
    }

    #[test]
    fn format_difficulty_handles_invalid_or_unusable_bits() {
        assert_eq!(P2PoolStatusView::format_difficulty(""), "");
        assert_eq!(
            P2PoolStatusView::format_difficulty("not-a-compact-bits-value"),
            "not...alue"
        );
        assert_eq!(P2PoolStatusView::format_difficulty("1d000000"), "-");
        assert_eq!(P2PoolStatusView::format_difficulty("ff00ffff"), "-");
    }

    #[test]
    fn format_timestamp_formats_seconds_and_milliseconds() {
        assert_eq!(
            P2PoolStatusView::format_timestamp(0),
            "1/1/1970, 12:00:00 AM"
        );
        assert_eq!(
            P2PoolStatusView::format_timestamp(43_200),
            "1/1/1970, 12:00:00 PM"
        );
        assert_eq!(
            P2PoolStatusView::format_timestamp(1_700_000_000),
            "11/14/2023, 10:13:20 PM"
        );
        assert_eq!(
            P2PoolStatusView::format_timestamp(1_700_000_000_000),
            "11/14/2023, 10:13:20 PM"
        );
    }

    #[test]
    fn render_dispatches_process_for_tab_three() {
        let mut app = App::new();
        app.p2pool_status_tab = 3;

        let output = render_view(&app);

        assert!(output.contains("P2Poolv2 Process"));
        assert!(output.contains("[s] Start"));
        assert!(output.contains("[t] Stop"));
        assert!(output.contains("[r] Restart"));
        assert!(output.contains("Process state: Stopped"));
    }

    #[test]
    fn handle_process_input_start_when_stopped() {
        use crossterm::event::KeyModifiers;

        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());

        let action = P2PoolStatusView::handle_process_input(&mut app, key);

        assert!(matches!(action, AppAction::StartP2Pool));
    }

    #[test]
    fn handle_process_input_ignores_stop_when_already_stopped() {
        use crossterm::event::KeyModifiers;

        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::empty());

        let action = P2PoolStatusView::handle_process_input(&mut app, key);

        assert!(matches!(action, AppAction::None));
    }

    #[test]
    fn handle_process_input_ignores_unrelated_keys() {
        use crossterm::event::KeyModifiers;

        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty());

        let action = P2PoolStatusView::handle_process_input(&mut app, key);

        assert!(matches!(action, AppAction::None));
    }
}
