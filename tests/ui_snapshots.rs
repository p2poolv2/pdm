// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use pdm::app::{App, CurrentScreen};
use pdm::ui::ui;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn test_home_screen_render() {
    //  Setup App State
    let mut app = App::new();
    app.current_screen = CurrentScreen::Home;

    //  Setup Test Terminal (80x25 standard size)
    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();

    //  Render
    terminal.draw(|f| ui(f, &mut app)).unwrap();

    //  Snapshot the buffer
    // This asserts that the "drawing commands" sent to the terminal remain consistent.
    insta::assert_debug_snapshot!(terminal.backend());
}

#[test]
fn test_config_screen_render() {
    let mut app = App::new();
    // Simulate user navigating to Config
    app.sidebar_index = 1;
    app.toggle_menu();

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| ui(f, &mut app)).unwrap();

    insta::assert_debug_snapshot!(terminal.backend());
}
