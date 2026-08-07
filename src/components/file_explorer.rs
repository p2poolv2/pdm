// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::fs;
use std::path::PathBuf;

/// `FileExplorer` maintains the current directory, a sorted list of entries,
/// and the currently selected index. It supports navigating directories,
/// moving the selection, and selecting files.
#[derive(Clone)]
pub struct FileExplorer {
    /// Current directory being explored.
    pub current_dir: PathBuf,
    /// Sorted list of files and folders in `current_dir`.
    pub files: Vec<PathBuf>,
    /// Index of the currently selected item.
    pub selected_index: usize,
    /// When true, the explorer is in directory-selection mode.
    pub allow_dir_select: bool,
}

impl Default for FileExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl FileExplorer {
    /// Creates a new `FileExplorer` starting at the process working directory.
    #[must_use]
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut explorer = Self {
            current_dir,
            files: Vec::new(),
            selected_index: 0,
            allow_dir_select: false,
        };
        explorer.load_directory();
        explorer
    }

    /// Loads the contents of `current_dir` into `files`.
    ///
    /// Directories are listed first, followed by files. If the directory
    /// has a parent, a virtual `..` entry is added to allow navigating upward.
    pub fn load_directory(&mut self) {
        self.files.clear();
        self.selected_index = 0;

        if self.allow_dir_select {
            self.files.push(self.current_dir.clone());
        }

        if self.current_dir.parent().is_some() {
            self.files.push(self.current_dir.join(".."));
        }

        if let Ok(entries) = fs::read_dir(&self.current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if !self.allow_dir_select {
                    files.push(path);
                }
            }

            dirs.sort();
            files.sort();

            self.files.append(&mut dirs);
            self.files.append(&mut files);
        }
    }

    /// Moves the selection to the next entry.
    pub fn next(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.files.len();
        }
    }

    /// Moves the selection to the previous entry.
    pub fn previous(&mut self) {
        if !self.files.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.files.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    /// Selects the current entry.
    ///
    /// - If it is a directory, enters that directory.
    /// - If it is `..`, moves to the parent directory.
    /// - If it is a file, returns its path.
    pub fn select(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            return None;
        }

        let selected = self.files[self.selected_index].clone();

        if self.allow_dir_select && selected == self.current_dir {
            return Some(selected);
        }

        if selected.ends_with("..") {
            if let Some(parent) = self.current_dir.parent() {
                self.current_dir = parent.to_path_buf();
                self.load_directory();
            }
        } else if selected.is_dir() {
            self.current_dir = selected;
            self.load_directory();
        } else {
            return Some(selected);
        }

        None
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Up => {
                self.previous();
                AppAction::None
            }
            KeyCode::Down => {
                self.next();
                AppAction::None
            }
            KeyCode::Enter => {
                if let Some(path) = self.select() {
                    return AppAction::FileSelected(path);
                }
                AppAction::None
            }
            KeyCode::Backspace => {
                if let Some(parent) = self.current_dir.parent() {
                    self.current_dir = parent.to_path_buf();
                    self.load_directory();
                }
                AppAction::None
            }
            KeyCode::Esc => AppAction::CloseModal,
            _ => AppAction::None,
        }
    }

    pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
        let allow_dir_select = app.explorer.allow_dir_select;
        let sentinel = app.explorer.current_dir.clone();

        let files: Vec<ListItem> = app
            .explorer
            .files
            .iter()
            .map(|path| {
                let display_name = if allow_dir_select && path == &sentinel {
                    "[✓ Use this directory]".to_string()
                } else if path.ends_with("..") {
                    "📁 ..".to_string()
                } else {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if path.is_dir() {
                        format!("📁 {name}")
                    } else {
                        format!("📄 {name}")
                    }
                };
                ListItem::new(display_name)
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(app.explorer.selected_index));

        let title = if allow_dir_select {
            format!(
                " Select Directory (Current: {}) ",
                app.explorer.current_dir.display()
            )
        } else {
            format!(
                " Select File (Current: {}) ",
                app.explorer.current_dir.display()
            )
        };

        let list = List::new(files)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn setup_temp_fs() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut base = std::env::temp_dir();

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        base.push(format!("pdm_file_explorer_test_{}", unique));

        fs::create_dir_all(&base).unwrap();
        fs::create_dir(base.join("folder")).unwrap();
        File::create(base.join("file.txt")).unwrap();

        base
    }

    #[test]
    fn loads_directory_entries() {
        let dir = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: dir,
            files: vec![],
            selected_index: 0,
            allow_dir_select: false,
        };

        explorer.load_directory();
        assert!(explorer.files.len() >= 2);
    }

    #[test]
    fn next_and_previous_wrap() {
        let dir = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: dir,
            files: vec![PathBuf::from("a"), PathBuf::from("b")],
            selected_index: 0,
            allow_dir_select: false,
        };

        explorer.next();
        assert_eq!(explorer.selected_index, 1);

        explorer.next();
        assert_eq!(explorer.selected_index, 0);

        explorer.previous();
        assert_eq!(explorer.selected_index, 1);
    }

    #[test]
    fn selecting_file_returns_path() {
        let dir = setup_temp_fs();
        let file = dir.join("file.txt");

        let mut explorer = FileExplorer {
            current_dir: dir,
            files: vec![file.clone()],
            selected_index: 0,
            allow_dir_select: false,
        };

        let result = explorer.select();
        assert_eq!(result, Some(file));
    }

    #[test]
    fn selecting_parent_directory_moves_up() {
        let base = setup_temp_fs();
        let child = base.join("child");
        fs::create_dir(&child).unwrap();

        let mut explorer = FileExplorer {
            current_dir: child.clone(),
            files: vec![],
            selected_index: 0,
            allow_dir_select: false,
        };

        explorer.load_directory();

        // First entry must be ".."
        assert!(explorer.files[0].ends_with(".."));

        // Select the ".." entry
        let result = explorer.select();

        // It should move to parent and not return a file
        assert!(result.is_none());
        assert_eq!(explorer.current_dir, base);
        assert!(!explorer.files.is_empty());
    }

    #[test]
    fn selecting_directory_enters_directory() {
        let base = setup_temp_fs();
        let folder = base.join("folder");

        let mut explorer = FileExplorer {
            current_dir: base.clone(),
            files: vec![folder.clone()],
            selected_index: 0,
            allow_dir_select: false,
        };

        let result = explorer.select();
        assert!(result.is_none());
        assert_eq!(explorer.current_dir, folder);
    }

    #[test]
    fn default_constructs_explorer() {
        let explorer = FileExplorer::default();
        assert!(!explorer.current_dir.as_os_str().is_empty());
    }

    #[test]
    fn previous_decrements_when_not_zero() {
        let dir = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: dir,
            files: vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")],
            selected_index: 2,
            allow_dir_select: false,
        };

        explorer.previous();
        assert_eq!(explorer.selected_index, 1);
    }

    #[test]
    fn backspace_navigates_to_parent_directory() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let base = setup_temp_fs();
        let child = base.join("folder");

        let mut explorer = FileExplorer {
            current_dir: child.clone(),
            files: vec![],
            selected_index: 0,
            allow_dir_select: false,
        };
        explorer.load_directory();

        let action =
            explorer.handle_input(KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()));
        assert!(matches!(action, crate::app::AppAction::None));
        assert_eq!(explorer.current_dir, base);
    }

    #[test]
    fn esc_returns_close_modal() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let dir = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: dir,
            files: vec![],
            selected_index: 0,
            allow_dir_select: false,
        };

        let action = explorer.handle_input(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()));
        assert!(matches!(action, crate::app::AppAction::CloseModal));
    }

    #[test]
    fn allow_dir_select_prepends_sentinel_and_hides_files() {
        let base = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: base.clone(),
            files: Vec::new(),
            selected_index: 0,
            allow_dir_select: true,
        };
        explorer.load_directory();

        // First entry must be the sentinel.
        assert_eq!(explorer.files[0], base, "sentinel must be first");
        // Regular files must be excluded.
        assert!(
            explorer
                .files
                .iter()
                .all(|p| p.extension().is_none_or(|e| e != "txt")),
            "txt files must be hidden in dir-select mode"
        );
        // Subdirectories must still appear.
        assert!(
            explorer
                .files
                .iter()
                .any(|p| p.file_name() == Some(std::ffi::OsStr::new("folder")))
        );
    }

    #[test]
    fn allow_dir_select_sentinel_returns_current_dir() {
        let base = setup_temp_fs();
        let mut explorer = FileExplorer {
            current_dir: base.clone(),
            files: Vec::new(),
            selected_index: 0,
            allow_dir_select: true,
        };
        explorer.load_directory();
        // Select index 0 (sentinel)
        let result = explorer.select();
        assert_eq!(result, Some(base));
    }

    #[test]
    fn allow_dir_select_subdir_still_navigates() {
        let base = setup_temp_fs();
        let folder = base.join("folder");

        let mut explorer = FileExplorer {
            current_dir: base.clone(),
            files: Vec::new(),
            selected_index: 0,
            allow_dir_select: true,
        };
        explorer.load_directory();

        // Find and select the "folder" subdirectory entry.
        let folder_idx = explorer
            .files
            .iter()
            .position(|p| p == &folder)
            .expect("folder entry must exist");
        explorer.selected_index = folder_idx;
        let result = explorer.select();

        // Navigates into the subdir, does not return it.
        assert!(result.is_none());
        assert_eq!(explorer.current_dir, folder);
    }

    #[test]
    fn render_displays_files_and_dirs() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let base = setup_temp_fs();
        let mut app = App::new();
        app.explorer.current_dir = base.clone();
        app.explorer.load_directory();

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                FileExplorer::render(f, &mut app, area);
            })
            .unwrap();

        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();

        // The title bar and at least one entry indicator must be present
        assert!(output.contains("Select File"));
        // The folder and file created by setup_temp_fs must appear with icons
        assert!(output.contains("folder") || output.contains("📁"));
        assert!(output.contains("file.txt") || output.contains("📄"));
    }
}
