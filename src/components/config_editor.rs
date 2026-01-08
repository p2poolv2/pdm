// SPDX-FileCopyrightText: 2024 PDM Authors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::p2pool_parser::ConfigSection;
use ratatui::widgets::ListState;

pub struct ConfigEditor {
    pub sections: Vec<ConfigSection>,
    pub selected_tab: usize,
    pub field_state: ListState,
}

impl ConfigEditor {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            selected_tab: 0,
            field_state: ListState::default(),
        }
    }

    /// Loads parsed data into the editor
    pub fn load_data(&mut self, sections: Vec<ConfigSection>) {
        self.sections = sections;
        self.selected_tab = 0;
        self.field_state.select(None);

        // Auto-select first field if available
        if let Some(first) = self.sections.first() {
            if !first.fields.is_empty() {
                self.field_state.select(Some(0));
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.sections.is_empty() {
            return;
        }
        self.selected_tab = (self.selected_tab + 1) % self.sections.len();
        self.reset_field_selection();
    }

    pub fn previous_tab(&mut self) {
        if self.sections.is_empty() {
            return;
        }
        if self.selected_tab == 0 {
            self.selected_tab = self.sections.len() - 1;
        } else {
            self.selected_tab -= 1;
        }
        self.reset_field_selection();
    }

    pub fn next_field(&mut self) {
        if self.sections.is_empty() {
            return;
        }
        let count = self.sections[self.selected_tab].fields.len();
        if count == 0 {
            return;
        }

        let i = match self.field_state.selected() {
            Some(i) => (i + 1) % count,
            None => 0,
        };
        self.field_state.select(Some(i));
    }

    pub fn previous_field(&mut self) {
        if self.sections.is_empty() {
            return;
        }
        let count = self.sections[self.selected_tab].fields.len();
        if count == 0 {
            return;
        }

        let i = match self.field_state.selected() {
            Some(i) => {
                if i == 0 {
                    count - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.field_state.select(Some(i));
    }

    fn reset_field_selection(&mut self) {
        if !self.sections[self.selected_tab].fields.is_empty() {
            self.field_state.select(Some(0));
        } else {
            self.field_state.select(None);
        }
    }
}
impl Default for ConfigEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::p2pool_parser::{ConfigField, ConfigSection};

    fn make_editor() -> ConfigEditor {
        let mut editor = ConfigEditor::new();
        editor.load_data(vec![
            ConfigSection {
                title: "A".into(),
                fields: vec![
                    ConfigField {
                        key: "k1".into(),
                        value: "v1".into(),
                    },
                    ConfigField {
                        key: "k2".into(),
                        value: "v2".into(),
                    },
                ],
            },
            ConfigSection {
                title: "B".into(),
                fields: vec![ConfigField {
                    key: "x".into(),
                    value: "y".into(),
                }],
            },
        ]);
        editor
    }

    #[test]
    fn load_data_autoselects_first_field() {
        let editor = make_editor();
        assert_eq!(editor.selected_tab, 0);
        assert_eq!(editor.field_state.selected(), Some(0));
    }

    #[test]
    fn next_tab_wraps_and_resets_field() {
        let mut editor = make_editor();
        editor.next_tab();
        assert_eq!(editor.selected_tab, 1);
        assert_eq!(editor.field_state.selected(), Some(0));

        editor.next_tab();
        assert_eq!(editor.selected_tab, 0);
    }

    #[test]
    fn previous_tab_wraps() {
        let mut editor = make_editor();
        editor.previous_tab();
        assert_eq!(editor.selected_tab, 1);
    }

    #[test]
    fn next_field_cycles() {
        let mut editor = make_editor();
        editor.next_field();
        assert_eq!(editor.field_state.selected(), Some(1));

        editor.next_field();
        assert_eq!(editor.field_state.selected(), Some(0));
    }

    #[test]
    fn previous_field_cycles() {
        let mut editor = make_editor();
        editor.previous_field();
        assert_eq!(editor.field_state.selected(), Some(1));
    }

    #[test]
    fn empty_sections_safe_no_panic() {
        let mut editor = ConfigEditor::new();
        editor.next_tab();
        editor.previous_tab();
        editor.next_field();
        editor.previous_field();
        // Should not panic
    }

    #[test]
    fn empty_fields_safe_no_panic() {
        let mut editor = ConfigEditor::new();
        editor.load_data(vec![ConfigSection {
            title: "Empty".into(),
            fields: vec![],
        }]);

        editor.next_field();
        editor.previous_field();
        assert_eq!(editor.field_state.selected(), None);
    }

    #[test]
    fn default_constructs_clean_editor() {
        let editor = ConfigEditor::default();
        assert!(editor.sections.is_empty());
        assert_eq!(editor.selected_tab, 0);
        assert_eq!(editor.field_state.selected(), None);
    }
}
