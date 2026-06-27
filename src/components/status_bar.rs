// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, CurrentScreen};
use crate::components::settings_view::{FIELDS, FieldKind};
use ratatui::{prelude::*, widgets::Paragraph};

#[derive(Clone, Debug)]
pub struct StatusBar;

fn hint(key: &str, desc: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {key} "),
            Style::default().bg(Color::DarkGray).fg(Color::White),
        ),
        Span::styled(format!(" {desc}  "), Style::default().fg(Color::DarkGray)),
    ]
}

impl StatusBar {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    // Status bar
    pub fn render(f: &mut Frame, app: &App, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();

        match app.current_screen {
            CurrentScreen::FileExplorer => {
                spans.extend(hint("↑↓", "Navigate"));
                spans.extend(hint("Enter", "Select"));
                spans.extend(hint("⌫", "Parent folder"));
                spans.extend(hint("Esc", "Cancel"));
            }
            CurrentScreen::BitcoinConfig if app.bitcoin_conf_path.is_some() => {
                if let Some(msg) = &app.bitcoin_config_view.save_message {
                    spans.push(Span::styled(
                        format!(" ✓ {msg}  "),
                        Style::default().fg(Color::Green),
                    ));
                } else if app.bitcoin_config_view.editing {
                    spans.extend(hint("Enter", "Confirm"));
                    spans.extend(hint("Esc", "Cancel"));
                } else if app.bitcoin_config_view.sidebar_focused {
                    spans.extend(hint("↑↓", "Navigate sidebar"));
                    spans.extend(hint("Enter", "Focus config"));
                } else {
                    spans.extend(hint("↑↓", "Navigate"));
                    spans.extend(hint("Enter", "Edit"));
                    spans.extend(hint("s", "Save"));
                    spans.extend(hint("Esc", "Back"));
                }
            }
            CurrentScreen::P2PoolConfig if app.p2pool_conf_path.is_some() => {
                spans.extend(hint("↑↓", "Navigate"));
                spans.extend(hint("Enter", "Open file"));
                spans.extend(hint("q", "Quit"));
            }
            CurrentScreen::BitcoinConfig => {
                if let Some(msg) = &app.bitcoin_config_view.warning_message {
                    spans.push(Span::styled(
                        format!(" ⚠ {msg}  "),
                        Style::default().fg(Color::Yellow),
                    ));
                    spans.extend(hint("Enter", "Try again"));
                } else {
                    spans.extend(hint("↑↓", "Navigate sidebar"));
                    spans.extend(hint("Enter", "Open file"));
                    spans.extend(hint("Esc", "Back"));
                }
            }
            CurrentScreen::Settings => {
                if let Some(err) = &app.settings_view.save_error {
                    spans.push(Span::styled(
                        format!(" ⚠ {err}  "),
                        Style::default().fg(Color::Red),
                    ));
                } else if app.settings_view.sidebar_focused {
                    spans.extend(hint("↑↓", "Navigate sidebar"));
                    spans.extend(hint("Enter", "Focus settings"));
                } else {
                    let s = &app.settings;
                    let idx = app.settings_view.selected_index;
                    let field_is_set = match idx {
                        0 => s.bitcoin_conf_path.is_some(),
                        1 => s.p2pool_conf_path.is_some(),
                        2 => s.ln_conf_path.is_some(),
                        3 => s.shares_market_conf_path.is_some(),
                        4 => s.settings_dir_override.is_some(),
                        _ => false,
                    };
                    spans.extend(hint("↑↓", "Navigate"));
                    if let Some(&(_, kind)) = FIELDS.get(idx) {
                        let label = if matches!(kind, FieldKind::DirectoryPicker) {
                            "Browse dir"
                        } else {
                            "Browse file"
                        };
                        spans.extend(hint("Enter", label));
                    }
                    if field_is_set {
                        spans.extend(hint("⌫", "Clear"));
                    }
                    spans.extend(hint("Esc", "Back"));
                }
            }
            CurrentScreen::BitcoinStatus | CurrentScreen::P2PoolStatus => {
                spans.extend(hint("↑↓", "Navigate sidebar"));
                spans.extend(hint("←→", "Switch tab"));
                spans.extend(hint("q", "Quit"));
            }
            _ => {
                spans.extend(hint("↑↓", "Navigate sidebar"));
                spans.extend(hint("Enter", "Select"));
                spans.extend(hint("q", "Quit"));
            }
        }

        let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
        f.render_widget(bar, area);
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, CurrentScreen};
    use ratatui::{Terminal, backend::TestBackend};

    fn render_status_bar(app: &App) -> String {
        let backend = TestBackend::new(120, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                StatusBar::render(f, app, area);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn file_explorer_shows_navigate_and_cancel() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::FileExplorer;
        let output = render_status_bar(&app);
        assert!(output.contains("Navigate"));
        assert!(output.contains("Cancel"));
        assert!(output.contains("Parent folder"));
    }

    #[test]
    fn bitcoin_config_no_file_shows_open_file() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        let output = render_status_bar(&app);
        assert!(output.contains("Open file"));
    }

    #[test]
    fn bitcoin_config_no_file_with_warning_shows_try_again() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        app.bitcoin_config_view.warning_message = Some("Not a valid config.".to_string());
        let output = render_status_bar(&app);
        assert!(output.contains("Not a valid config."));
        assert!(output.contains("Try again"));
    }

    #[test]
    fn bitcoin_config_with_file_sidebar_focused_shows_navigate_sidebar() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        app.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_config_view.sidebar_focused = true;
        let output = render_status_bar(&app);
        assert!(output.contains("Navigate sidebar"));
        assert!(output.contains("Focus config"));
    }

    #[test]
    fn bitcoin_config_with_file_editing_shows_confirm_cancel() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        app.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_config_view.sidebar_focused = false;
        app.bitcoin_config_view.editing = true;
        let output = render_status_bar(&app);
        assert!(output.contains("Confirm"));
        assert!(output.contains("Cancel"));
    }

    #[test]
    fn bitcoin_config_with_file_browsing_shows_edit_save_back() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        app.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_config_view.sidebar_focused = false;
        app.bitcoin_config_view.editing = false;
        let output = render_status_bar(&app);
        assert!(output.contains("Edit"));
        assert!(output.contains("Save"));
        assert!(output.contains("Back"));
    }

    #[test]
    fn bitcoin_config_with_file_save_message_shows_saved() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinConfig;
        app.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        app.bitcoin_config_view.save_message = Some("Configuration correctly saved".to_string());
        let output = render_status_bar(&app);
        assert!(output.contains("Configuration correctly saved"));
    }

    #[test]
    fn bitcoin_status_shows_switch_tab() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::BitcoinStatus;
        let output = render_status_bar(&app);
        assert!(output.contains("Switch tab"));
    }

    #[test]
    fn default_screen_shows_select() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Home;
        let output = render_status_bar(&app);
        assert!(output.contains("Select"));
    }

    #[test]
    fn settings_sidebar_focused_shows_navigate_sidebar() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = true;
        let output = render_status_bar(&app);
        assert!(output.contains("Navigate sidebar"));
        assert!(output.contains("Focus settings"));
    }

    #[test]
    fn settings_content_focused_shows_browse_back() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        // field 0 is FilePicker
        app.settings_view.selected_index = 0;
        let output = render_status_bar(&app);
        assert!(output.contains("Browse file"));
        assert!(output.contains("Back"));
    }

    #[test]
    fn settings_content_focused_dir_field_shows_browse_dir() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        // field 4 is DirectoryPicker
        app.settings_view.selected_index = 4;
        let output = render_status_bar(&app);
        assert!(output.contains("Browse dir"));
        assert!(output.contains("Back"));
    }

    #[test]
    fn settings_content_focused_field_set_shows_clear_hint() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 0;
        app.settings.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_focused_field_unset_no_clear_hint() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 0;
        // bitcoin_conf_path is None by default
        let output = render_status_bar(&app);
        assert!(!output.contains("Clear"));
    }

    #[test]
    fn settings_save_error_shown_in_status_bar() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.save_error = Some("disk full".to_string());
        let output = render_status_bar(&app);
        assert!(output.contains("disk full"));
    }

    #[test]
    fn settings_content_p2pool_field_set_shows_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 1;
        app.settings.p2pool_conf_path = Some(std::path::PathBuf::from("/tmp/p2pool.toml"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_ln_field_set_shows_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 2;
        app.settings.ln_conf_path = Some(std::path::PathBuf::from("/tmp/ln.conf"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_shares_field_set_shows_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 3;
        app.settings.shares_market_conf_path = Some(std::path::PathBuf::from("/tmp/shares.conf"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_out_of_range_field_no_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 99;
        let output = render_status_bar(&app);
        assert!(!output.contains("Clear"));
    }

    #[test]
    fn settings_content_directory_override_field_set_shows_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 4;
        app.settings.settings_dir_override = Some(std::path::PathBuf::from("/custom/dir"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }
}
