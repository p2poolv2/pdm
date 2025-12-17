// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct FileExplorer {
    pub current_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub selected_index: usize,
}

impl Default for FileExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl FileExplorer {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut explorer = Self {
            current_dir,
            files: Vec::new(),
            selected_index: 0,
        };
        explorer.load_directory();
        explorer
    }

    pub fn load_directory(&mut self) {
        self.files.clear();
        self.selected_index = 0;

        // Add ".." for going up a directory
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
                } else {
                    files.push(path);
                }
            }

            dirs.sort();
            files.sort();

            self.files.append(&mut dirs);
            self.files.append(&mut files);
        }
    }

    pub fn next(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.files.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.files.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.files.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn select(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            return None;
        }

        let selected = self.files[self.selected_index].clone();

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
}
