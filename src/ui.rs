// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, CurrentScreen};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Sidebar
            Constraint::Min(0),     // Main Content
        ])
        .split(f.area());

    //  Sidebar
    let items = vec![ListItem::new("Home"), ListItem::new("Bitcoin Config")];

    // Highlight the active one
    let mut state = ListState::default();
    state.select(Some(app.sidebar_index));

    let sidebar = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" PDM "))
        .highlight_style(Style::default().bg(Color::Gray).fg(Color::Black));

    f.render_stateful_widget(sidebar, chunks[0], &mut state);

    // Main Content
    let main_area = chunks[1];

    match app.current_screen {
        CurrentScreen::Home => {
            let p = Paragraph::new("Welcome to PDM.\nPress 'q' to quit.")
                .block(Block::default().borders(Borders::ALL).title(" Home "));
            f.render_widget(p, main_area);
        }
        CurrentScreen::BitcoinConfig => {
            // File Explorer
            let p = Paragraph::new("[ Load Bitcoin Config ]").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Bitcoin Config "),
            );
            f.render_widget(p, main_area);
        }
        _ => {}
    }
}
