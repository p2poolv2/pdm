// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::config::{ConfigEntry, ConfigType, parse_config};
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Position, Rect},
    widgets::ListState,
};
use std::path::PathBuf;

#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub enum CurrentScreen {
    #[default]
    Home,
    BitcoinConfig,
    FileExplorer,
    Editing,
    EditingValue,
    // Exiting,
}

#[derive(Debug, Default)]
pub struct InteractiveRects {
    pub select_config_button: Option<Rect>,
    pub file_list: Option<Rect>,
    pub tabs: Option<Rect>,
    pub config_list: Option<Rect>,
}

#[derive(Debug, Clone)]
pub struct FileExplorerState {
    pub current_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub list_state: ListState,
}

impl Default for FileExplorerState {
    fn default() -> Self {
        Self::new()
    }
}

impl FileExplorerState {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut state = Self {
            current_dir,
            files: vec![],
            list_state: ListState::default(),
        };
        state.refresh_files();
        state.list_state.select(Some(0));
        state
    }

    pub fn refresh_files(&mut self) {
        self.files.clear();
        if self.current_dir.parent().is_some() {
            self.files.push(self.current_dir.join(".."));
        }

        if let Ok(entries) = std::fs::read_dir(&self.current_dir) {
            for entry in entries.flatten() {
                self.files.push(entry.path());
            }
        }
        self.files.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        if self.files.is_empty() {
            self.list_state.select(None);
        } else {
            let selected = self.list_state.selected().unwrap_or(0);
            if selected >= self.files.len() {
                self.list_state.select(Some(self.files.len() - 1));
            } else {
                self.list_state.select(Some(selected));
            }
        }
    }

    pub fn select_next(&mut self) {
        if self.files.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.files.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn select_previous(&mut self) {
        if self.files.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.files.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
}

#[derive(Debug, Clone)]
pub struct ConfigSection {
    pub name: String,
    pub items: Vec<ConfigEntry>,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub sidebar_index: usize,
    pub config_flow_return_screen: CurrentScreen,
    pub sections: Vec<ConfigSection>,
    pub selected_section_index: usize,
    pub selected_item_index: usize,
    pub editing_value: String,
    pub config_list_state: ListState,
    pub config_file_path: Option<String>,
    pub notification: Option<String>,
    pub interactive_rects: InteractiveRects,
    pub file_explorer: FileExplorerState,
    pub running: bool,
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::Home,
            sidebar_index: 0,
            config_flow_return_screen: CurrentScreen::Home,
            sections: vec![],
            selected_section_index: 0,
            selected_item_index: 0,
            editing_value: String::new(),
            config_list_state: ListState::default(),
            config_file_path: None,
            notification: None,
            interactive_rects: InteractiveRects::default(),
            file_explorer: FileExplorerState::new(),
            running: true,
        }
    }

    pub fn toggle_menu(&mut self) {
        // Logic to switch between sidebar items
        match self.sidebar_index {
            0 => self.current_screen = CurrentScreen::Home,
            1 => self.current_screen = CurrentScreen::BitcoinConfig,
            _ => {}
        }
    }

    pub fn load_config(&mut self, path: &str) -> Result<()> {
        let entries = parse_config(std::path::Path::new(path))?;

        let mut sections_map: std::collections::HashMap<String, Vec<ConfigEntry>> =
            std::collections::HashMap::new();

        for entry in entries {
            let section_name = if let Some(schema) = &entry.schema {
                schema.section.clone()
            } else {
                "Custom".to_string()
            };
            sections_map.entry(section_name).or_default().push(entry);
        }

        self.sections = sections_map
            .into_iter()
            .map(|(name, items)| ConfigSection { name, items })
            .collect();

        self.sections.sort_by(|a, b| a.name.cmp(&b.name));

        self.selected_section_index = 0;
        self.selected_item_index = 0;
        self.config_list_state.select(Some(0));

        Ok(())
    }

    pub fn save_config(&mut self) -> Result<()> {
        if let Some(path) = &self.config_file_path {
            let mut content = String::new();
            for section in &self.sections {
                if section.items.iter().any(|i| i.enabled) {
                    content.push_str(&format!("# Section: {}\n", section.name));
                    for entry in &section.items {
                        if entry.enabled {
                            content.push_str(&format!("{}={}\n", entry.key, entry.value));
                        }
                    }
                    content.push('\n');
                }
            }
            std::fs::write(path, content)?;
            self.notification = Some("Configuration saved successfully.".to_string());
        }
        Ok(())
    }

    pub fn handle_mouse_event(&mut self, mouse: event::MouseEvent) {
        if mouse.kind == event::MouseEventKind::Down(event::MouseButton::Left) {
            match self.current_screen {
                CurrentScreen::Home => {
                    if let Some(rect) = self.interactive_rects.select_config_button
                        && rect.contains(Position {
                            x: mouse.column,
                            y: mouse.row,
                        })
                    {
                        self.config_flow_return_screen = self.current_screen.clone();
                        self.current_screen = CurrentScreen::FileExplorer;
                        self.file_explorer.refresh_files();
                    }
                }
                CurrentScreen::BitcoinConfig => {
                    if let Some(rect) = self.interactive_rects.select_config_button
                        && rect.contains(Position {
                            x: mouse.column,
                            y: mouse.row,
                        })
                    {
                        self.config_flow_return_screen = self.current_screen.clone();
                        self.current_screen = CurrentScreen::FileExplorer;
                        self.file_explorer.refresh_files();
                    }
                }
                CurrentScreen::FileExplorer => {
                    if let Some(rect) = self.interactive_rects.file_list
                        && rect.contains(Position {
                            x: mouse.column,
                            y: mouse.row,
                        })
                        && mouse.row > rect.y
                        && mouse.row < rect.y + rect.height - 1
                    {
                        let offset = self.file_explorer.list_state.offset();
                        let row_index = (mouse.row as usize).saturating_sub(rect.y as usize + 1);
                        let index = row_index + offset;

                        if index < self.file_explorer.files.len() {
                            self.file_explorer.list_state.select(Some(index));
                        }
                    }
                }
                CurrentScreen::Editing => {
                    if let Some(rect) = self.interactive_rects.tabs
                        && rect.contains(Position {
                            x: mouse.column,
                            y: mouse.row,
                        })
                        && mouse.row > rect.y
                        && mouse.row < rect.y + rect.height - 1
                    {
                        let mut x = rect.x + 1;
                        for (i, section) in self.sections.iter().enumerate() {
                            let width = section.name.len() as u16 + 3;
                            if mouse.column >= x && mouse.column < x + width {
                                self.selected_section_index = i;
                                self.selected_item_index = 0;
                                self.config_list_state.select(Some(0));
                                break;
                            }
                            x += width + 1;
                        }
                    }

                    if let Some(rect) = self.interactive_rects.config_list
                        && rect.contains(Position {
                            x: mouse.column,
                            y: mouse.row,
                        })
                        && mouse.row > rect.y
                        && mouse.row < rect.y + rect.height - 1
                    {
                        let offset = self.config_list_state.offset();
                        let row_index = (mouse.row as usize).saturating_sub(rect.y as usize + 1);
                        let index = row_index + offset;

                        if let Some(section) = self.sections.get(self.selected_section_index)
                            && index < section.items.len()
                        {
                            self.selected_item_index = index;
                            self.config_list_state.select(Some(index));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        let is_ctrl_s =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s');
        if !is_ctrl_s {
            self.notification = None;
        }

        match self.current_screen {
            // Home Screen
            CurrentScreen::Home => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.running = false,
                _ => {}
            },
            // Bitcoin config Screen
            CurrentScreen::BitcoinConfig => match key.code {
                KeyCode::Char('q') | KeyCode::Char('?') => {}
                KeyCode::Enter => {
                    self.config_flow_return_screen = self.current_screen.clone();
                    self.current_screen = CurrentScreen::FileExplorer;
                    self.file_explorer.refresh_files();
                }
                _ => {}
            },
            // File Explorer Screen
            CurrentScreen::FileExplorer => match key.code {
                KeyCode::Esc => {
                    self.current_screen = self.config_flow_return_screen.clone();
                }
                KeyCode::Up => self.file_explorer.select_previous(),
                KeyCode::Down => self.file_explorer.select_next(),
                KeyCode::Enter => {
                    if let Some(selected_index) = self.file_explorer.list_state.selected()
                        && let Some(selected) = self.file_explorer.files.get(selected_index)
                    {
                        if selected.file_name().and_then(|n| n.to_str()) == Some("..")
                            || selected.ends_with("..")
                        {
                            if let Some(parent) = self.file_explorer.current_dir.parent() {
                                self.file_explorer.current_dir = parent.to_path_buf();
                                self.file_explorer.list_state.select(Some(0));
                                self.file_explorer.refresh_files();
                            }
                        } else if selected.is_dir() {
                            self.file_explorer.current_dir = selected.clone();
                            self.file_explorer.list_state.select(Some(0));
                            self.file_explorer.refresh_files();
                        } else {
                            let path_str = selected.to_string_lossy().to_string();
                            self.config_file_path = Some(path_str.clone());
                            if self.load_config(&path_str).is_err() {
                            } else {
                                self.current_screen = CurrentScreen::Editing;
                            }
                        }
                    }
                }
                _ => {}
            },
            // Editing Screen
            CurrentScreen::Editing => {
                if is_ctrl_s {
                    let _ = self.save_config();
                    return;
                }
                match key.code {
                    KeyCode::Esc => {
                        self.current_screen = self.config_flow_return_screen.clone();
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        if !self.sections.is_empty() {
                            if self.selected_section_index < self.sections.len() - 1 {
                                self.selected_section_index += 1;
                            } else {
                                self.selected_section_index = 0;
                            }
                            self.selected_item_index = 0;
                            self.config_list_state.select(Some(0));
                        }
                    }
                    KeyCode::Left | KeyCode::BackTab => {
                        if !self.sections.is_empty() {
                            if self.selected_section_index > 0 {
                                self.selected_section_index -= 1;
                            } else {
                                self.selected_section_index = self.sections.len() - 1;
                            }
                            self.selected_item_index = 0;
                            self.config_list_state.select(Some(0));
                        }
                    }
                    KeyCode::Down => {
                        if let Some(section) = self.sections.get(self.selected_section_index)
                            && !section.items.is_empty()
                            && self.selected_item_index < section.items.len() - 1
                        {
                            self.selected_item_index += 1;
                            self.config_list_state
                                .select(Some(self.selected_item_index));
                        }
                    }
                    KeyCode::Up => {
                        if self.selected_item_index > 0 {
                            self.selected_item_index -= 1;
                            self.config_list_state
                                .select(Some(self.selected_item_index));
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(section) = self.sections.get_mut(self.selected_section_index)
                            && let Some(entry) = section.items.get_mut(self.selected_item_index)
                        {
                            let is_bool = if let Some(schema) = &entry.schema {
                                schema.value_type == ConfigType::Boolean
                            } else {
                                false
                            };

                            if is_bool {
                                entry.value = if entry.value == "1" {
                                    "0".into()
                                } else {
                                    "1".into()
                                };
                                entry.enabled = true;
                            } else {
                                self.editing_value = entry.value.clone();
                                self.current_screen = CurrentScreen::EditingValue;
                            }
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(section) = self.sections.get_mut(self.selected_section_index)
                            && let Some(entry) = section.items.get_mut(self.selected_item_index)
                        {
                            entry.enabled = !entry.enabled;
                        }
                    }
                    _ => {}
                }
            }
            // Editing value popup
            CurrentScreen::EditingValue => {
                if is_ctrl_s {
                    if let Some(section) = self.sections.get_mut(self.selected_section_index)
                        && let Some(entry) = section.items.get_mut(self.selected_item_index)
                    {
                        entry.value = self.editing_value.clone();
                        entry.enabled = true;
                    }
                    let _ = self.save_config();
                    self.current_screen = CurrentScreen::Editing;
                    return;
                }

                match key.code {
                    KeyCode::Esc => {
                        self.current_screen = CurrentScreen::Editing;
                    }
                    KeyCode::Enter => {
                        if let Some(section) = self.sections.get_mut(self.selected_section_index)
                            && let Some(entry) = section.items.get_mut(self.selected_item_index)
                        {
                            entry.value = self.editing_value.clone();
                            entry.enabled = true;
                        }
                        self.current_screen = CurrentScreen::Editing;
                    }
                    KeyCode::Backspace => {
                        self.editing_value.pop();
                    }
                    KeyCode::Char(c) => {
                        self.editing_value.push(c);
                    }
                    _ => {}
                }
            }
        }
    }
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
