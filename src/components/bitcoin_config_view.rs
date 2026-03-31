// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction};
use crate::config::{ConfigEntry, CATEGORY_ORDER};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

/// A group of config entries under one section header.
#[derive(Debug, Clone)]
pub struct SectionGroup {
    pub category: String,
    pub entry_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct BitcoinConfigView {
    /// Flat index across all entries (not visual rows with headers).
    pub selected_index: usize,
    /// Vertical scroll offset in visual rows.
    pub scroll_offset: usize,
    /// Whether we are currently editing a value.
    pub edit_mode: bool,
    /// The in-progress text while editing.
    pub edit_buffer: String,
    /// Character cursor position within edit_buffer.
    pub edit_cursor_pos: usize,
    /// Section layout built from entries.
    pub section_layout: Vec<SectionGroup>,
    /// Total number of entries across all sections.
    pub total_entries: usize,
}

impl BitcoinConfigView {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            scroll_offset: 0,
            edit_mode: false,
            edit_buffer: String::new(),
            edit_cursor_pos: 0,
            section_layout: Vec::new(),
            total_entries: 0,
        }
    }

    /// Rebuild section layout from the current entries.
    pub fn rebuild_sections(&mut self, entries: &[ConfigEntry]) {
        self.section_layout.clear();

        for &category in CATEGORY_ORDER {
            let indices: Vec<usize> = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.schema
                        .as_ref()
                        .is_some_and(|s| s.category == category)
                })
                .map(|(i, _)| i)
                .collect();

            if !indices.is_empty() {
                self.section_layout.push(SectionGroup {
                    category: category.to_string(),
                    entry_indices: indices,
                });
            }
        }

        // "Other" entries (no schema)
        let other_indices: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.schema.is_none())
            .map(|(i, _)| i)
            .collect();

        if !other_indices.is_empty() {
            self.section_layout.push(SectionGroup {
                category: "Other".to_string(),
                entry_indices: other_indices,
            });
        }

        self.total_entries = entries.len();
        if self.selected_index >= self.total_entries && self.total_entries > 0 {
            self.selected_index = self.total_entries - 1;
        }
    }

    /// Convert a flat entry index to the data index in entries via section_layout.
    /// Returns the (section_idx, position_within_section, entry_data_index).
    fn flat_to_data(&self, flat_idx: usize) -> Option<(usize, usize, usize)> {
        let mut count = 0;
        for (sec_idx, section) in self.section_layout.iter().enumerate() {
            if flat_idx < count + section.entry_indices.len() {
                let pos = flat_idx - count;
                return Some((sec_idx, pos, section.entry_indices[pos]));
            }
            count += section.entry_indices.len();
        }
        None
    }

    /// Get the data index for the currently selected flat index.
    fn selected_data_index(&self) -> Option<usize> {
        self.flat_to_data(self.selected_index).map(|(_, _, idx)| idx)
    }

    /// Compute the visual row for a given flat entry index.
    /// Visual rows include section headers (1 row each) plus entries.
    fn visual_row_for(&self, flat_idx: usize) -> usize {
        let mut visual = 0;
        let mut count = 0;
        for section in &self.section_layout {
            visual += 1; // header row
            if flat_idx < count + section.entry_indices.len() {
                visual += flat_idx - count;
                return visual;
            }
            visual += section.entry_indices.len();
            count += section.entry_indices.len();
        }
        visual
    }

    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        entries: &mut [ConfigEntry],
    ) -> AppAction {
        if self.edit_mode {
            return self.handle_edit_input(key, entries);
        }

        match key.code {
            KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                AppAction::None
            }
            KeyCode::Down => {
                if self.total_entries > 0 && self.selected_index < self.total_entries - 1 {
                    self.selected_index += 1;
                }
                AppAction::None
            }
            KeyCode::Enter => {
                if let Some(data_idx) = self.selected_data_index() {
                    self.edit_mode = true;
                    self.edit_buffer = entries[data_idx].value.clone();
                    self.edit_cursor_pos = self.edit_buffer.len();
                }
                AppAction::None
            }
            KeyCode::Char('s') => AppAction::SaveConfig,
            KeyCode::Esc => AppAction::CloseModal,
            _ => AppAction::None,
        }
    }

    fn handle_edit_input(
        &mut self,
        key: KeyEvent,
        entries: &mut [ConfigEntry],
    ) -> AppAction {
        match key.code {
            KeyCode::Char(c) => {
                self.edit_buffer.insert(self.edit_cursor_pos, c);
                self.edit_cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.edit_cursor_pos > 0 {
                    self.edit_cursor_pos -= 1;
                    self.edit_buffer.remove(self.edit_cursor_pos);
                }
            }
            KeyCode::Delete => {
                if self.edit_cursor_pos < self.edit_buffer.len() {
                    self.edit_buffer.remove(self.edit_cursor_pos);
                }
            }
            KeyCode::Left => {
                self.edit_cursor_pos = self.edit_cursor_pos.saturating_sub(1);
            }
            KeyCode::Right => {
                if self.edit_cursor_pos < self.edit_buffer.len() {
                    self.edit_cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.edit_cursor_pos = 0;
            }
            KeyCode::End => {
                self.edit_cursor_pos = self.edit_buffer.len();
            }
            KeyCode::Enter => {
                // Confirm edit
                if let Some(data_idx) = self.selected_data_index() {
                    entries[data_idx].value = self.edit_buffer.clone();
                    entries[data_idx].enabled = true;
                }
                self.edit_mode = false;
                self.edit_buffer.clear();
                self.edit_cursor_pos = 0;
            }
            KeyCode::Esc => {
                // Cancel edit
                self.edit_mode = false;
                self.edit_buffer.clear();
                self.edit_cursor_pos = 0;
            }
            _ => {}
        }
        AppAction::None
    }

    pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
        if app.bitcoin_conf_path.is_some() {
            let view = &mut app.bitcoin_config_view;
            let entries = &app.bitcoin_data;

            if entries.is_empty() {
                let p = Paragraph::new("No configuration entries found.").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Bitcoin Configuration "),
                );
                f.render_widget(p, area);
                return;
            }

            // Build visual lines
            let mut lines: Vec<Line> = Vec::new();
            let selected_data_idx = view.selected_data_index();

            for section in &view.section_layout {
                // Section header
                let header = Line::from(Span::styled(
                    format!("── {} ──", section.category),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
                lines.push(header);

                // Entries in this section
                for &entry_idx in &section.entry_indices {
                    let entry = &entries[entry_idx];
                    let is_selected = selected_data_idx == Some(entry_idx);

                    let line = if is_selected && view.edit_mode {
                        // Render edit buffer with cursor
                        let buf = &view.edit_buffer;
                        let pos = view.edit_cursor_pos;
                        let before_cursor = &buf[..pos];
                        let (cursor_char, after_cursor) = if pos < buf.len()
                        {
                            let next =
                                pos + buf[pos..].chars().next().unwrap().len_utf8();
                            (
                                buf[pos..next].to_string(),
                                &buf[next..],
                            )
                        } else {
                            (" ".to_string(), "")
                        };

                        Line::from(vec![
                            Span::styled(
                                format!("  {} = ", entry.key),
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::DarkGray),
                            ),
                            Span::styled(
                                before_cursor.to_string(),
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::DarkGray),
                            ),
                            Span::styled(
                                cursor_char,
                                Style::default()
                                    .fg(Color::Black)
                                    .bg(Color::White),
                            ),
                            Span::styled(
                                after_cursor.to_string(),
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::DarkGray),
                            ),
                        ])
                    } else {
                        let base_style = if !entry.enabled {
                            Style::default().fg(Color::DarkGray)
                        } else {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        };

                        let bg_style = if is_selected {
                            base_style.bg(Color::DarkGray)
                        } else {
                            base_style
                        };

                        let mut spans = vec![
                            Span::styled(
                                format!("  {} = ", entry.key),
                                bg_style,
                            ),
                            Span::styled(&entry.value, bg_style),
                        ];
                        if !entry.enabled {
                            spans.push(Span::styled(
                                " (disabled)",
                                bg_style,
                            ));
                        }
                        Line::from(spans)
                    };

                    lines.push(line);
                }
            }

            // Adjust scroll offset to keep selection visible
            let content_height = area.height.saturating_sub(2) as usize; // borders
            if view.total_entries > 0 {
                let selected_visual =
                    view.visual_row_for(view.selected_index);
                if selected_visual < view.scroll_offset {
                    view.scroll_offset = selected_visual;
                } else if selected_visual
                    >= view.scroll_offset + content_height
                {
                    view.scroll_offset =
                        selected_visual - content_height + 1;
                }
            }

            let hint = if view.edit_mode {
                " [Enter] Save  [Esc] Cancel "
            } else {
                " [↑↓] Navigate  [Enter] Edit  [s] Save file  [Esc] Back "
            };

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Bitcoin Configuration ")
                        .title_bottom(Line::from(hint).alignment(Alignment::Center)),
                )
                .scroll((view.scroll_offset as u16, 0));

            f.render_widget(paragraph, area);
        } else {
            let p = Paragraph::new(
                "Press [Enter] to select a bitcoin.conf file",
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Bitcoin Config "),
            );
            f.render_widget(p, area);
        }
    }
}

impl Default for BitcoinConfigView {
    fn default() -> Self {
        Self::new()
    }
}

