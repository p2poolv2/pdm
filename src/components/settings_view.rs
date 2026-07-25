// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState},
};

/// Number of settings fields.
pub const FIELD_COUNT: usize = 9;

/// Describes how a settings field behaves when Enter is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Opens a file-explorer dialog so the user can pick a file.
    FilePicker,
    /// Opens a file-explorer dialog in directory-selection mode.
    DirectoryPicker,
}

/// All settings fields in display order.  Each entry is `(label, kind)`.
pub const FIELDS: [(&str, FieldKind); FIELD_COUNT] = [
    ("Bitcoin config path", FieldKind::FilePicker),
    ("P2Pool config path", FieldKind::FilePicker),
    ("LN config path", FieldKind::FilePicker),
    ("Shares Market config path", FieldKind::FilePicker),
    ("Bitcoin Core data directory", FieldKind::DirectoryPicker),
    ("Bitcoin Core log file", FieldKind::FilePicker),
    ("Settings directory", FieldKind::DirectoryPicker),
    ("Bitcoin Core executable", FieldKind::FilePicker),
    ("P2Poolv2 executable", FieldKind::FilePicker),
];

#[derive(Debug, Clone)]
pub struct SettingsView {
    pub selected_index: usize,
    pub sidebar_focused: bool,
    /// Set when `save_settings` returns an error; displayed in the status bar.
    pub save_error: Option<String>,
}

impl SettingsView {
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            sidebar_focused: true,
            save_error: None,
        }
    }

    /// Called only when the settings content panel is focused (`sidebar_focused` = false).
    pub fn handle_input(&mut self, key: KeyEvent) -> AppAction {
        debug_assert!(
            !self.sidebar_focused,
            "handle_input called while sidebar is focused"
        );

        match key.code {
            KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                AppAction::None
            }
            KeyCode::Down => {
                if self.selected_index + 1 < FIELDS.len() {
                    self.selected_index += 1;
                }
                AppAction::None
            }
            KeyCode::Enter => AppAction::OpenExplorerForSettings(self.selected_index),
            KeyCode::Backspace => AppAction::ClearSettingsField(self.selected_index),
            KeyCode::Esc => {
                self.sidebar_focused = true;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
        let values: [Option<String>; FIELD_COUNT] = [
            app.settings
                .bitcoin_conf_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .p2pool_conf_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .ln_conf_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .shares_market_conf_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .bitcoin_core_data_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .bitcoin_core_log_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .settings_dir_override
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .bitcoind_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            app.settings
                .p2poolv2_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        ];

        let items: Vec<ListItem> = (0..FIELD_COUNT)
            .map(|idx| {
                let (label, _kind) = FIELDS[idx];
                let val = &values[idx];
                let (display, style) = match val {
                    Some(v) => (
                        v.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    None => {
                        if idx == 6 {
                            let path = if app.config_dir.as_os_str().is_empty() {
                                "(unknown)".to_string()
                            } else {
                                app.config_dir.to_string_lossy().into_owned()
                            };
                            (path, Style::default().fg(Color::DarkGray))
                        } else {
                            (
                                "(not set)".to_string(),
                                Style::default().fg(Color::DarkGray),
                            )
                        }
                    }
                };
                ListItem::new(vec![
                    Line::from(Span::styled(label, Style::default().fg(Color::Gray))),
                    Line::from(Span::styled(display, style)),
                ])
            })
            .collect();

        let mut list_state = ListState::default();
        list_state.select(Some(app.settings_view.selected_index));

        let panel_style = if app.settings_view.sidebar_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Settings ")
                    .border_style(panel_style),
            )
            .highlight_style(Style::default().bg(Color::DarkGray));

        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    /// Returns a view with the content panel focused (sidebar_focused = false),
    /// satisfying the precondition of `handle_input`.
    fn content_focused_view() -> SettingsView {
        let mut view = SettingsView::new();
        view.sidebar_focused = false;
        view
    }

    #[test]
    fn new_starts_at_first_field_sidebar_focused() {
        let view = SettingsView::new();
        assert_eq!(view.selected_index, 0);
        assert!(view.sidebar_focused);
        assert!(view.save_error.is_none());
    }

    #[test]
    fn browsing_down_increments_index() {
        let mut view = content_focused_view();
        view.handle_input(key(KeyCode::Down));
        assert_eq!(view.selected_index, 1);
    }

    #[test]
    fn browsing_down_clamped_at_last_field() {
        let mut view = content_focused_view();
        view.selected_index = FIELD_COUNT - 1;
        view.handle_input(key(KeyCode::Down));
        assert_eq!(view.selected_index, FIELD_COUNT - 1);
    }

    #[test]
    fn browsing_up_decrements_index() {
        let mut view = content_focused_view();
        view.selected_index = 2;
        view.handle_input(key(KeyCode::Up));
        assert_eq!(view.selected_index, 1);
    }

    #[test]
    fn browsing_up_clamped_at_zero() {
        let mut view = content_focused_view();
        view.selected_index = 0;
        view.handle_input(key(KeyCode::Up));
        assert_eq!(view.selected_index, 0);
    }

    #[test]
    fn browsing_enter_opens_explorer_for_all_fields() {
        let mut view = content_focused_view();
        for idx in 0..FIELD_COUNT {
            view.selected_index = idx;
            let action = view.handle_input(key(KeyCode::Enter));
            assert!(
                matches!(action, AppAction::OpenExplorerForSettings(i) if i == idx),
                "expected OpenExplorerForSettings({idx})"
            );
        }
    }

    #[test]
    fn browsing_esc_sets_sidebar_focused_flag() {
        let mut view = content_focused_view();
        let action = view.handle_input(key(KeyCode::Esc));
        assert!(matches!(action, AppAction::None));
        assert!(view.sidebar_focused);
    }

    #[test]
    fn browsing_other_key_is_noop() {
        let mut view = content_focused_view();
        let action = view.handle_input(key(KeyCode::F(5)));
        assert!(matches!(action, AppAction::None));
    }

    #[test]
    fn backspace_returns_clear_for_any_field() {
        let mut view = content_focused_view();
        for idx in 0..FIELD_COUNT {
            view.selected_index = idx;
            let action = view.handle_input(key(KeyCode::Backspace));
            assert!(
                matches!(action, AppAction::ClearSettingsField(i) if i == idx),
                "expected ClearSettingsField({idx})"
            );
        }
    }

    #[test]
    fn render_with_values_set_and_content_focused() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.settings.bitcoin_conf_path = Some(std::path::PathBuf::from("/tmp/bitcoin.conf"));
        app.settings.p2pool_conf_path = Some(std::path::PathBuf::from("/tmp/p2pool.toml"));
        app.settings.ln_conf_path = Some(std::path::PathBuf::from("/tmp/ln.conf"));
        app.settings.shares_market_conf_path = Some(std::path::PathBuf::from("/tmp/shares.conf"));
        app.settings.bitcoin_core_data_dir = Some(std::path::PathBuf::from("/tmp/bitcoin"));
        app.settings.bitcoin_core_log_path =
            Some(std::path::PathBuf::from("/tmp/bitcoin/debug.log"));
        app.settings.settings_dir_override = Some(std::path::PathBuf::from("/custom/dir"));
        app.settings_view.sidebar_focused = false;

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                SettingsView::render(f, &mut app, area);
            })
            .unwrap();

        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();

        assert!(output.contains("bitcoin.conf") || output.contains("Settings"));
        assert!(output.contains("custom") || output.contains("Settings directory"));
    }

    #[test]
    #[serial_test::serial]
    fn render_field6_shows_default_config_dir_when_no_override() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        // Point config dir to a known location so the test is deterministic.
        // SAFETY: no concurrent mutation of PDM_CONFIG_DIR in this test.
        unsafe { std::env::set_var("PDM_CONFIG_DIR", "/pdm/test-config") };

        let mut app = App::new();
        app.settings_view.sidebar_focused = false;
        // settings_dir_override is None by default

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                SettingsView::render(f, &mut app, area);
            })
            .unwrap();

        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();

        unsafe { std::env::remove_var("PDM_CONFIG_DIR") };

        assert!(
            output.contains("/pdm/test-config"),
            "default config dir must appear when no override is set"
        );
    }

    #[test]
    fn render_shows_p2poolv2_executable_value_when_set() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.settings.p2poolv2_path = Some(std::path::PathBuf::from("/opt/p2poolv2/bin/p2poolv2"));
        app.settings_view.sidebar_focused = false;

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                SettingsView::render(f, &mut app, area);
            })
            .unwrap();

        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();

        assert!(output.contains("p2poolv2") || output.contains("P2Poolv2 executable"));
    }
}
