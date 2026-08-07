// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction};
use crate::p2poolv2_config::{FieldKind, P2PoolConfigEntry, flatten_config};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

#[derive(Debug, Clone)]
pub struct P2PoolConfigView {
    pub selected_index: usize,
    pub editing: bool,
    pub edit_input: String,
    pub save_message: Option<String>,
    pub warning_message: Option<String>,
    pub sidebar_focused: bool,
}

/// Returns `(display_string, style)` for a config entry value.
pub fn entry_display(entry: &P2PoolConfigEntry) -> (String, Style) {
    if entry.enabled {
        let v = if entry.schema.sensitive {
            "••••••••".to_string()
        } else {
            entry.value.clone()
        };
        (
            v,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        let placeholder = match &entry.schema.kind {
            FieldKind::Optional { default: Some(d) } => format!("default: {}", d),
            _ => "not set".to_string(),
        };
        (
            format!("({})", placeholder),
            Style::default().fg(Color::DarkGray),
        )
    }
}

/// Returns the edit-mode display string (masked if sensitive).
pub fn edit_display(input: &str, sensitive: bool) -> String {
    if sensitive {
        "•".repeat(input.len()) + "_"
    } else {
        format!("{}_", input)
    }
}

impl P2PoolConfigView {
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            editing: false,
            edit_input: String::new(),
            save_message: None,
            warning_message: None,
            sidebar_focused: true,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent, entries: &[P2PoolConfigEntry]) -> AppAction {
        self.save_message = None;

        if self.editing {
            match key.code {
                KeyCode::Enter => {
                    let action =
                        AppAction::CommitP2PoolEdit(self.selected_index, self.edit_input.clone());
                    self.editing = false;
                    self.edit_input.clear();
                    action
                }
                KeyCode::Esc => {
                    self.editing = false;
                    self.edit_input.clear();
                    AppAction::None
                }
                KeyCode::Backspace => {
                    self.edit_input.pop();
                    AppAction::None
                }
                KeyCode::Char(c) => {
                    self.edit_input.push(c);
                    AppAction::None
                }
                _ => AppAction::None,
            }
        } else {
            match key.code {
                KeyCode::Up => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                    }
                    AppAction::None
                }
                KeyCode::Down => {
                    if self.selected_index + 1 < entries.len() {
                        self.selected_index += 1;
                    }
                    AppAction::None
                }
                KeyCode::Enter => {
                    if !entries.is_empty() {
                        self.edit_input = entries[self.selected_index].value.clone();
                        self.editing = true;
                    }
                    AppAction::None
                }
                KeyCode::Char('s') => AppAction::SaveP2PoolConfig,
                KeyCode::Esc => {
                    self.sidebar_focused = true;
                    AppAction::None
                }
                _ => AppAction::None,
            }
        }
    }

    pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
        if app.p2pool_conf_path.is_none() {
            // Show warning if there is one,
            let msg = app
                .p2pool_config_view
                .warning_message
                .as_deref()
                .unwrap_or("Press [Enter] to select a p2poolv2 config file");

            let style = if app.p2pool_config_view.warning_message.is_some() {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };

            let p = Paragraph::new(msg).style(style).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" P2Pool Config "),
            );
            f.render_widget(p, area);
            return;
        }

        let entries: Vec<P2PoolConfigEntry> = app
            .p2pool_config
            .as_ref()
            .map(flatten_config)
            .unwrap_or_default();

        // Status bar (warning or save message)
        // Warning (red) takes priority over save message (green).
        let status_message = app
            .p2pool_config_view
            .warning_message
            .as_deref()
            .map(|msg| (msg, Style::default().fg(Color::Red)))
            .or_else(|| {
                app.p2pool_config_view
                    .save_message
                    .as_deref()
                    .map(|msg| (msg, Style::default().fg(Color::Green)))
            });

        // If there is a message, crave a 3-row strip off the top and render the rest of the view beneath it.
        let content_area = if let Some((msg, style)) = status_message {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(area);

            let status = Paragraph::new(msg)
                .style(style)
                .block(Block::default().borders(Borders::ALL).title(" Status "));
            f.render_widget(status, layout[0]);
            layout[1]
        } else {
            area
        };

        // Split panel layout
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(content_area);

        // Left panel: scrollable entry list
        let items: Vec<ListItem> = entries
            .iter()
            .map(|entry| {
                let (value_display, value_style) = entry_display(entry);

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            format!("[{}] ", entry.section),
                            Style::default().fg(Color::Blue),
                        ),
                        Span::styled(
                            entry.schema.description.as_str(),
                            Style::default().fg(Color::Gray),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("{} = ", entry.key),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(value_display, value_style),
                    ]),
                ])
            })
            .collect();

        let mut list_state = ListState::default();
        list_state.select(Some(app.p2pool_config_view.selected_index));

        let title = match &app.p2pool_conf_path {
            Some(path) => format!(" P2Pool Configuration --- {} ", path.display()),
            None => " P2Pool Configuration ".to_string(),
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::DarkGray));

        f.render_stateful_widget(list, panels[0], &mut list_state);

        // Right panel: detail + edit
        let right_block = Block::default().borders(Borders::ALL).title(" Detail ");
        let inner = right_block.inner(panels[1]);
        f.render_widget(right_block, panels[1]);

        let selected_entry = entries.get(app.p2pool_config_view.selected_index);
        let editing = app.p2pool_config_view.editing;
        let edit_input = app.p2pool_config_view.edit_input.clone();

        if let Some(entry) = selected_entry {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // description
                    Constraint::Length(1), // type + section
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // "Value:" label
                    Constraint::Length(3), // value / input box
                    Constraint::Length(1), // sensitive notice
                    Constraint::Min(0),
                ])
                .split(inner);

            f.render_widget(
                Paragraph::new(entry.schema.description.as_str())
                    .style(Style::default().fg(Color::White)),
                rows[0],
            );
            f.render_widget(
                Paragraph::new(format!(
                    "[{}]  type: {}",
                    entry.section, entry.schema.type_hint
                ))
                .style(Style::default().fg(Color::Gray)),
                rows[1],
            );
            f.render_widget(
                Paragraph::new("Value:").style(Style::default().fg(Color::Gray)),
                rows[3],
            );

            if editing {
                f.render_widget(
                    Paragraph::new(edit_display(&edit_input, entry.schema.sensitive))
                        .block(Block::default().borders(Borders::ALL))
                        .style(Style::default().fg(Color::Yellow)),
                    rows[4],
                );
            } else {
                let (display, style) = entry_display(entry);
                f.render_widget(
                    Paragraph::new(display)
                        .block(Block::default().borders(Borders::ALL))
                        .style(style),
                    rows[4],
                );
            }

            if entry.schema.sensitive {
                f.render_widget(
                    Paragraph::new("⚠ sensitive field").style(Style::default().fg(Color::Yellow)),
                    rows[5],
                );
            }
        }
    }
}

impl Default for P2PoolConfigView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2poolv2_config::{ConfigSection, FieldKind, P2PoolConfigEntry, P2PoolFieldSchema};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use p2poolv2_config::Config;
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;

    fn make_entry(key: &str, value: &str, enabled: bool) -> P2PoolConfigEntry {
        P2PoolConfigEntry {
            section: ConfigSection::Stratum,
            key: key.to_string(),
            value: value.to_string(),
            enabled,
            schema: P2PoolFieldSchema {
                description: "test field".to_string(),
                kind: FieldKind::Required,
                type_hint: "String".to_string(),
                sensitive: false,
            },
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn make_config() -> Config {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[stratum]
hostname = "127.0.0.1"
port = 3333
start_difficulty = 1000
minimum_difficulty = 100
maximum_difficulty = 100000
zmqpubhashblock = "tcp://127.0.0.1:28332"
bootstrap_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
network = "signet"
version_mask = "1fffe000"
difficulty_multiplier = 1.0
pool_signature = "MyPool/1.0"

[bitcoinrpc]
url = "http://127.0.0.1:38332"
username = "rpcuser"
password = "rpcpassword"

[network]
listen_address = "0.0.0.0:8333"
dial_peers = []
max_pending_incoming = 10
max_pending_outgoing = 10
max_established_incoming = 50
max_established_outgoing = 50
max_established_per_peer = 1
max_workbase_per_second = 10
max_userworkbase_per_second = 10
max_miningshare_per_second = 100
max_inventory_per_second = 100
max_transaction_per_second = 100
max_requests_per_second = 1
dial_timeout_secs = 30

[store]
path = "./data/store"
background_task_frequency_hours = 1
pplns_ttl_days = 7

[logging]
level = "info"
stats_dir = "./logs/stats"
console = true

[api]
hostname = "127.0.0.1"
port = 3030
        "#,
        )
        .unwrap();

        // keep dir alive until Config is loaded
        Config::load(path.to_str().unwrap()).expect("inline test config must parse")
    }

    #[test]
    fn editing_char_appends() {
        let mut view = P2PoolConfigView::new();
        view.editing = true;
        let entries = vec![make_entry("hostname", "old", true)];
        view.handle_input(key(KeyCode::Char('x')), &entries);
        assert_eq!(view.edit_input, "x");
    }

    #[test]
    fn editing_backspace_removes_last_char() {
        let mut view = P2PoolConfigView::new();
        view.editing = true;
        view.edit_input = "ab".to_string();
        let entries = vec![make_entry("hostname", "old", true)];
        view.handle_input(key(KeyCode::Backspace), &entries);
        assert_eq!(view.edit_input, "a");
    }

    #[test]
    fn editing_enter_returns_commit_action() {
        let mut view = P2PoolConfigView::new();
        view.editing = true;
        view.edit_input = "newval".to_string();
        view.selected_index = 0;
        let entries = vec![make_entry("hostname", "old", true)];
        let action = view.handle_input(key(KeyCode::Enter), &entries);
        assert!(
            matches!(action, AppAction::CommitP2PoolEdit(0, ref v) if v == "newval"),
            "expected CommitP2PoolEdit(0, newval)"
        );
        assert!(!view.editing);
        assert!(view.edit_input.is_empty());
    }

    #[test]
    fn editing_esc_cancels() {
        let mut view = P2PoolConfigView::new();
        view.editing = true;
        view.edit_input = "draft".to_string();
        let entries = vec![make_entry("hostname", "old", true)];
        let action = view.handle_input(key(KeyCode::Esc), &entries);
        assert!(matches!(action, AppAction::None));
        assert!(!view.editing);
        assert!(view.edit_input.is_empty());
    }

    #[test]
    fn browsing_down_increments_index() {
        let mut view = P2PoolConfigView::new();
        let entries = vec![make_entry("a", "1", true), make_entry("b", "2", true)];
        view.handle_input(key(KeyCode::Down), &entries);
        assert_eq!(view.selected_index, 1);
    }

    #[test]
    fn browsing_down_clamped_at_last() {
        let mut view = P2PoolConfigView::new();
        view.selected_index = 1;
        let entries = vec![make_entry("a", "1", true), make_entry("b", "2", true)];
        view.handle_input(key(KeyCode::Down), &entries);
        assert_eq!(view.selected_index, 1);
    }

    #[test]
    fn browsing_up_decrements_index() {
        let mut view = P2PoolConfigView::new();
        view.selected_index = 1;
        let entries = vec![make_entry("a", "1", true), make_entry("b", "2", true)];
        view.handle_input(key(KeyCode::Up), &entries);
        assert_eq!(view.selected_index, 0);
    }

    #[test]
    fn browsing_up_clamped_at_zero() {
        let mut view = P2PoolConfigView::new();
        let entries = vec![make_entry("a", "1", true)];
        view.handle_input(key(KeyCode::Up), &entries);
        assert_eq!(view.selected_index, 0);
    }

    #[test]
    fn browsing_enter_starts_editing_with_current_value() {
        let mut view = P2PoolConfigView::new();
        let entries = vec![make_entry("hostname", "127.0.0.1", true)];
        view.handle_input(key(KeyCode::Enter), &entries);
        assert!(view.editing);
        assert_eq!(view.edit_input, "127.0.0.1");
    }

    #[test]
    fn browsing_enter_noop_on_empty_entries() {
        let mut view = P2PoolConfigView::new();
        view.handle_input(key(KeyCode::Enter), &[]);
        assert!(!view.editing);
    }

    #[test]
    fn browsing_s_returns_save_action() {
        let mut view = P2PoolConfigView::new();
        let entries = vec![make_entry("hostname", "127.0.0.1", true)];
        let action = view.handle_input(key(KeyCode::Char('s')), &entries);
        assert!(matches!(action, AppAction::SaveP2PoolConfig));
    }

    #[test]
    fn browsing_esc_sets_sidebar_focused() {
        let mut view = P2PoolConfigView::new();
        view.sidebar_focused = false;
        let entries = vec![make_entry("hostname", "127.0.0.1", true)];
        view.handle_input(key(KeyCode::Esc), &entries);
        assert!(view.sidebar_focused);
    }

    #[test]
    fn any_key_clears_save_message() {
        let mut view = P2PoolConfigView::new();
        view.save_message = Some("saved".to_string());
        let entries = vec![make_entry("hostname", "127.0.0.1", true)];
        view.handle_input(key(KeyCode::Up), &entries);
        assert!(view.save_message.is_none());
    }

    #[test]
    fn entry_display_enabled_non_sensitive() {
        let entry = make_entry("host", "127.0.0.1", true);
        let (display, style) = entry_display(&entry);
        assert_eq!(display, "127.0.0.1");
        assert_eq!(style.fg, Some(Color::White));
    }

    #[test]
    fn entry_display_enabled_sensitive() {
        let mut entry = make_entry("pass", "secret", true);
        entry.schema.sensitive = true;
        let (display, _) = entry_display(&entry);
        assert_eq!(display, "••••••••");
    }

    #[test]
    fn entry_display_disabled_with_default() {
        let mut entry = make_entry("port", "", false);
        entry.schema.kind = FieldKind::Optional {
            default: Some("3333".into()),
        };
        let (display, style) = entry_display(&entry);
        assert_eq!(display, "(default: 3333)");
        assert_eq!(style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn entry_display_disabled_no_default() {
        let entry = make_entry("port", "", false);
        let (display, _) = entry_display(&entry);
        assert_eq!(display, "(not set)");
    }

    #[test]
    fn edit_display_non_sensitive() {
        assert_eq!(edit_display("hello", false), "hello_");
    }

    #[test]
    fn edit_display_sensitive_masks_chars() {
        assert_eq!(edit_display("abc", true), "•••_");
    }

    #[test]
    fn edit_display_sensitive_empty_input() {
        assert_eq!(edit_display("", true), "_");
    }

    #[test]
    fn render_no_path_shows_prompt() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App {
            p2pool_conf_path: None,
            p2pool_config_view: P2PoolConfigView {
                warning_message: None,
                ..Default::default()
            },
            ..Default::default()
        };
        terminal
            .draw(|f| P2PoolConfigView::render(f, &mut app, f.area()))
            .unwrap();
        assert!(buffer_text(&terminal).contains("Press [Enter] to select"));
    }

    #[test]
    fn render_no_path_shows_warning_message() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App {
            p2pool_conf_path: None,
            p2pool_config_view: P2PoolConfigView {
                warning_message: Some("File not found".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        terminal
            .draw(|f| P2PoolConfigView::render(f, &mut app, f.area()))
            .unwrap();
        assert!(buffer_text(&terminal).contains("File not found"));
    }

    #[test]
    fn render_full_coverage() {
        use ratatui::{Terminal, backend::TestBackend};
        use std::path::PathBuf;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App {
            p2pool_conf_path: Some(PathBuf::from("test.toml")),
            ..Default::default()
        };

        let cfg = make_config();
        app.p2pool_config = Some(cfg.clone());

        let entries = flatten_config(&cfg);

        let sensitive_idx = entries.iter().position(|e| e.schema.sensitive).unwrap_or(0);

        app.p2pool_config_view.selected_index = sensitive_idx;

        app.p2pool_config_view.editing = true;
        app.p2pool_config_view.edit_input = "secret123".into();

        terminal
            .draw(|f| P2PoolConfigView::render(f, &mut app, f.area()))
            .unwrap();

        let text = buffer_text(&terminal);

        assert!(text.contains("P2Pool Configuration")); // list rendered
        assert!(text.contains("Value:")); // detail panel rendered
        assert!(text.contains("_")); // editing cursor present
    }
}
