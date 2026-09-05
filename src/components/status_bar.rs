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
            CurrentScreen::P2PoolConfig if app.p2pool_conf_path.is_some() => {
                spans.extend(hint("↑↓", "Navigate"));
                spans.extend(hint("Enter", "Open file"));
                spans.extend(hint("q", "Quit"));
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
                        0 => s.p2pool_conf_path.is_some(),
                        1 => s.settings_dir_override.is_some(),
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
        // field 1 is DirectoryPicker
        app.settings_view.selected_index = 1;
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
        app.settings.p2pool_conf_path = Some(std::path::PathBuf::from("/tmp/p2pool.toml"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_focused_field_unset_no_clear_hint() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 0;
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
        app.settings_view.selected_index = 0;
        app.settings.p2pool_conf_path = Some(std::path::PathBuf::from("/tmp/p2pool.toml"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }

    #[test]
    fn settings_content_directory_override_field_set_shows_clear() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::Settings;
        app.settings_view.sidebar_focused = false;
        app.settings_view.selected_index = 1;
        app.settings.settings_dir_override = Some(std::path::PathBuf::from("/custom/dir"));
        let output = render_status_bar(&app);
        assert!(output.contains("Clear"));
    }
}
