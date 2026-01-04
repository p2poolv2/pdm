// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, CurrentScreen};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
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
    let items = vec![
        ListItem::new("Home"),
        ListItem::new("Bitcoin Config"),
        ListItem::new("P2Pool Config"),
    ];

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
            let config_status = match &app.bitcoin_conf_path {
                Some(p) => format!("Loaded: {:?}", p),
                None => "No config loaded".to_string(),
            };

            let text = format!(
                "Welcome to PDM.\n\n{}\n\n(Navigate to 'Bitcoin Config' to load)",
                config_status
            );
            let p = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title(" Home "))
                .wrap(Wrap { trim: true });
            f.render_widget(p, main_area);
        }
        CurrentScreen::BitcoinConfig => {
            let p = Paragraph::new("Press [Enter] to select a bitcoin.conf file").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Bitcoin Config "),
            );
            f.render_widget(p, main_area);
        }
        CurrentScreen::P2PoolConfig => {
            if app.p2pool_conf_path.is_some() {
                render_config_editor(f, &mut app.p2pool_editor, main_area, "P2Pool Config");
            } else {
                let p = Paragraph::new("Press [Enter] to select a p2pool.conf file").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" P2Pool Config "),
                );
                f.render_widget(p, main_area);
            }
        }
        CurrentScreen::FileExplorer => {
            render_file_explorer(f, app, main_area);
        }
        _ => {}
    }
}

fn render_file_explorer(f: &mut Frame, app: &mut App, area: Rect) {
    let files: Vec<ListItem> = app
        .explorer
        .files
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let display_name = if path.is_dir() {
                format!("📁 {}", name)
            } else {
                format!("📄 {}", name)
            };
            ListItem::new(display_name)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.explorer.selected_index));

    let title = format!(" Select File (Current: {:?}) ", app.explorer.current_dir);

    let list = List::new(files)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut state);
}

fn render_config_editor(
    f: &mut Frame,
    editor: &mut crate::components::config_editor::ConfigEditor,
    area: Rect,
    title: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Tabs (Sections)
    let titles: Vec<Line> = editor
        .sections
        .iter()
        .map(|s| Line::from(s.title.clone()))
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} Sections ", title)),
        )
        .select(editor.selected_tab)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, chunks[0]);

    // Fields List
    if let Some(section) = editor.sections.get(editor.selected_tab) {
        let items: Vec<ListItem> = section
            .fields
            .iter()
            .map(|field| ListItem::new(format!("{} = {}", field.key, field.value)))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Edit Fields "),
            )
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, chunks[1], &mut editor.field_state);
    }
}
