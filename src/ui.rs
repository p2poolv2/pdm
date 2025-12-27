// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, CurrentScreen};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
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
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // content
            Constraint::Length(3), // footer
        ])
        .split(chunks[1]);

    let content_area = main_layout[0];
    let footer_area = main_layout[1];

    match app.current_screen {
        CurrentScreen::Home => {
            let p = Paragraph::new("Welcome to PDM.\nPress 'q' to quit.")
                .block(Block::default().borders(Borders::ALL).title(" Home "));
            f.render_widget(p, content_area);
        }
        CurrentScreen::BitcoinConfig => render_bitcoin_config(f, app, content_area),
        CurrentScreen::FileExplorer => render_file_explorer(f, app, content_area),
        CurrentScreen::Editing => render_editing_screen(f, app, content_area),
        CurrentScreen::EditingValue => {
            render_editing_screen(f, app, content_area); // Background
            render_editing_value_popup(f, app, content_area); // Popup
        }
    }

    render_footer(f, app, footer_area);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match app.current_screen {
        CurrentScreen::Home | CurrentScreen::BitcoinConfig => "↑/↓: menu  Enter: select  q: quit",
        CurrentScreen::FileExplorer => "↑/↓: navigate  Enter: open/select  Esc: back  q: quit",
        CurrentScreen::Editing => {
            "↑/↓: select  Tab/Shift+Tab: change section  Space: enable  Enter: edit/toggle  Ctrl+S: save  Esc: home  q: quit"
        }
        CurrentScreen::EditingValue => {
            "Type: edit  Backspace: delete  Enter: apply  Esc: cancel  Ctrl+S: save  q: quit"
        }
    };

    let mut spans: Vec<Span> = Vec::new();
    if let Some(note) = &app.notification {
        spans.push(Span::styled(
            note.clone(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(hints, Style::default().fg(Color::DarkGray)));

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, area);
}

fn render_bitcoin_config(frame: &mut Frame, app: &mut App, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Bitcoin Config ");
    frame.render_widget(outer.clone(), area);
    let inner = outer.inner(area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3), // Button height
            Constraint::Percentage(40),
        ])
        .split(inner);

    let centered_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(layout[1]);

    let button_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let button = Paragraph::new("Load Bitcoin Config")
        .block(Block::default().borders(Borders::ALL))
        .style(button_style)
        .alignment(Alignment::Center);
    frame.render_widget(button, centered_layout[1]);

    app.interactive_rects.select_config_button = Some(centered_layout[1]);
}

fn render_file_explorer(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .file_explorer
        .files
        .iter()
        .map(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
            let style = if path.is_dir() {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::White)
            };
            let display_name = if path.is_dir() {
                format!("{}/", name)
            } else {
                name.to_string()
            };
            ListItem::new(display_name).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "File Explorer: {}",
            app.file_explorer.current_dir.display()
        )))
        .highlight_style(
            Style::default()
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    frame.render_stateful_widget(list, area, &mut app.file_explorer.list_state);
    app.interactive_rects.file_list = Some(area);
}

fn render_editing_screen(frame: &mut Frame, app: &mut App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Main content
        ])
        .split(area);

    // Tabs
    let section_names: Vec<String> = app.sections.iter().map(|s| s.name.clone()).collect();
    let tabs = Tabs::new(section_names)
        .block(Block::default().borders(Borders::ALL).title("Sections"))
        .select(app.selected_section_index)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, layout[0]);
    app.interactive_rects.tabs = Some(layout[0]);

    if app.sections.is_empty() {
        let msg = Paragraph::new("No configuration loaded or empty.").alignment(Alignment::Center);
        frame.render_widget(msg, layout[1]);
        return;
    }

    let current_section = &app.sections[app.selected_section_index];

    // List (Left) and Details (Right)
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    // List of items
    let items: Vec<ListItem> = current_section
        .items
        .iter()
        .map(|entry| {
            let status = if entry.enabled { "[x]" } else { "[ ]" };
            let line = format!("{} {} = {}", status, entry.key, entry.value);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Options"))
        .highlight_style(
            Style::default()
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, content_layout[0], &mut app.config_list_state);
    app.interactive_rects.config_list = Some(content_layout[0]);

    // Details panel
    let selected_entry = current_section.items.get(app.selected_item_index);
    if let Some(entry) = selected_entry {
        let description = if let Some(schema) = &entry.schema {
            schema.description.as_str()
        } else {
            "Custom configuration option."
        };

        let type_info = if let Some(schema) = &entry.schema {
            format!("{:?}", schema.value_type)
        } else {
            "Unknown".to_string()
        };

        let details = [
            format!("Key: {}", entry.key),
            format!("Value: {}", entry.value),
            format!("Type: {}", type_info),
            String::new(),
            "Description:".to_string(),
            description.to_string(),
        ]
        .join("\n");

        let paragraph = Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, content_layout[1]);
    }
}

fn render_editing_value_popup(frame: &mut Frame, app: &App, area: Rect) {
    let area = centered_rect(60, 20, area);
    frame.render_widget(Clear, area);

    let block = Block::default().title("Edit Value").borders(Borders::ALL);
    let input = Paragraph::new(app.editing_value.as_str())
        .block(block)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(input, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
