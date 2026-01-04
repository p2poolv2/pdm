// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use pdm::app::{App, CurrentScreen};
use pdm::components::p2pool_parser::parse_p2pool_config;
use pdm::ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::Backend, backend::CrosstermBackend};
use std::io;

fn main() -> Result<()> {
    //  Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    //  Run App
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app, |_app: &mut App| event::read());

    //  Restore Terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

// Accept any Backend and an Event Provider Closure
fn run_app<B: Backend, F>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut event_provider: F,
) -> io::Result<()>
where
    F: FnMut(&mut App) -> io::Result<Event>,
{
    loop {
        terminal.draw(|f| ui::ui(f, app))?;

        // We check the event from our provider
        if let Event::Key(key) = event_provider(app)?
            && key.kind == KeyEventKind::Press
        {
            if key.code == KeyCode::Char('q') {
                return Ok(());
            }
            match app.current_screen {
                // File Explorer Modal
                CurrentScreen::FileExplorer => match key.code {
                    KeyCode::Up => app.explorer.previous(),
                    KeyCode::Down => app.explorer.next(),
                    KeyCode::Esc => {
                        if let Some(trigger) = app.explorer_trigger {
                            app.current_screen = trigger;
                        } else {
                            app.toggle_menu();
                        }
                    } // Cancel
                    KeyCode::Enter => {
                        if let Some(path) = app.explorer.select().filter(|p| p.is_file()) {
                            if let Some(trigger) = app.explorer_trigger {
                                match trigger {
                                    CurrentScreen::P2PoolConfig => {
                                        app.p2pool_conf_path = Some(path.clone());
                                        if let Ok(sections) = parse_p2pool_config(&path) {
                                            app.p2pool_editor.load_data(sections);
                                        }
                                        app.current_screen = CurrentScreen::P2PoolConfig;
                                    }
                                    CurrentScreen::BitcoinConfig => {
                                        app.bitcoin_conf_path = Some(path);
                                        app.current_screen = CurrentScreen::BitcoinConfig;
                                    }
                                    _ => {}
                                }
                            }
                            app.explorer_trigger = None;
                        }
                    }
                    _ => {}
                },

                // Standard Navigation
                _ => match key.code {
                    KeyCode::Enter => {
                        if app.current_screen == CurrentScreen::BitcoinConfig
                            || app.current_screen == CurrentScreen::P2PoolConfig
                        {
                            app.explorer_trigger = Some(app.current_screen);
                            app.current_screen = CurrentScreen::FileExplorer;
                        }
                    }

                    // P2POOL EDITOR KEYS
                    KeyCode::Right
                        if app.current_screen == CurrentScreen::P2PoolConfig
                            && app.p2pool_conf_path.is_some() =>
                    {
                        app.p2pool_editor.next_tab()
                    }

                    KeyCode::Left
                        if app.current_screen == CurrentScreen::P2PoolConfig
                            && app.p2pool_conf_path.is_some() =>
                    {
                        app.p2pool_editor.previous_tab()
                    }

                    KeyCode::Up
                        if app.current_screen == CurrentScreen::P2PoolConfig
                            && app.p2pool_conf_path.is_some() =>
                    {
                        app.p2pool_editor.previous_field()
                    }

                    KeyCode::Down
                        if app.current_screen == CurrentScreen::P2PoolConfig
                            && app.p2pool_conf_path.is_some() =>
                    {
                        app.p2pool_editor.next_field()
                    }

                    // SIDEBAR
                    KeyCode::Up => {
                        if app.sidebar_index > 0 {
                            app.sidebar_index -= 1;
                            app.toggle_menu();
                        }
                    }
                    KeyCode::Down => {
                        if app.sidebar_index < 2 {
                            app.sidebar_index += 1;
                            app.toggle_menu();
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState, KeyModifiers};
    use pdm::components::p2pool_parser::{ConfigField, ConfigSection};
    use ratatui::backend::TestBackend;
    use std::env::temp_dir;
    use std::fs::create_dir_all;

    #[test]
    fn test_app_integration_smoke_test() {
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        let mut step = 0;

        let event_provider = |_app: &mut App| {
            step += 1;
            match step {
                1 => Ok(Event::Key(KeyEvent {
                    code: KeyCode::Down,
                    modifiers: KeyModifiers::empty(),
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                })),
                2 => Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('q'),
                    modifiers: KeyModifiers::empty(),
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                })),
                _ => panic!("Should have exited"),
            }
        };

        // First frame
        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!("home_screen", terminal.backend());

        // Run app (process events + redraws)
        let res = run_app(&mut terminal, &mut app, event_provider);
        assert!(res.is_ok());

        // Final frame after DOWN
        insta::assert_debug_snapshot!("menu_toggled", terminal.backend());

        assert_eq!(app.sidebar_index, 1);
    }

    #[test]
    fn test_file_explorer_flow() {
        // Setup
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        // Define Steps
        let mut step = 0;
        let event_provider = |app: &mut App| {
            step += 1;
            match step {
                1 => {
                    // Start at Home.
                    // Action: Move DOWN to "Bitcoin Config"
                    Ok(Event::Key(KeyEvent::new(
                        KeyCode::Down,
                        KeyModifiers::empty(),
                    )))
                }
                2 => {
                    // Action: Press ENTER to open File Explorer
                    Ok(Event::Key(KeyEvent::new(
                        KeyCode::Enter,
                        KeyModifiers::empty(),
                    )))
                }
                3 => {
                    // WE ARE NOW IN FILE EXPLORER
                    // Assertion: Check internal state (safer than snapshotting dynamic file lists)
                    assert_eq!(
                        app.current_screen,
                        CurrentScreen::FileExplorer,
                        "Should have switched to File Explorer"
                    );

                    // Action: Move DOWN (Navigate file list)
                    Ok(Event::Key(KeyEvent::new(
                        KeyCode::Down,
                        KeyModifiers::empty(),
                    )))
                }
                4 => {
                    // Assertion: Check that selection moved
                    assert_eq!(
                        app.explorer.selected_index, 1,
                        "Should have selected the second file"
                    );

                    // Action: Press ESC to Cancel/Close
                    Ok(Event::Key(KeyEvent::new(
                        KeyCode::Esc,
                        KeyModifiers::empty(),
                    )))
                }
                5 => {
                    // BACK TO SIDEBAR
                    assert_eq!(
                        app.current_screen,
                        CurrentScreen::BitcoinConfig,
                        "Should have returned to Sidebar"
                    );

                    // Action: Quit
                    Ok(Event::Key(KeyEvent::new(
                        KeyCode::Char('q'),
                        KeyModifiers::empty(),
                    )))
                }
                _ => panic!("Step {} not handled", step),
            }
        };

        // Run
        let res = run_app(&mut terminal, &mut app, event_provider);
        assert!(res.is_ok());
    }

    #[test]
    fn test_file_explorer_wrap_and_select_sets_config() {
        use std::env::temp_dir;
        use std::fs::{File, create_dir_all};

        // Setup a temporary filesystem sandbox
        let base = temp_dir().join("pdm_select_test");
        let _ = std::fs::remove_dir_all(&base);
        create_dir_all(&base).unwrap();
        let file_path = base.join("bitcoin.conf");
        File::create(&file_path).unwrap();

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.explorer.current_dir = base.clone();
        app.explorer.load_directory();

        let mut step = 0;

        let event_provider = |app: &mut App| {
            step += 1;
            match step {
                1 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                ))), // move to bitcoin config
                2 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                ))), // open explorer
                3 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Up,
                    KeyModifiers::empty(),
                ))), // force wrap-around
                4 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                ))), // select file
                5 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Char('q'),
                    KeyModifiers::empty(),
                ))),
                _ => panic!("unexpected"),
            }
        };

        let res = run_app(&mut terminal, &mut app, event_provider);
        assert!(res.is_ok());

        assert_eq!(app.bitcoin_conf_path, Some(file_path));
    }

    #[test]
    fn test_p2pool_explorer_and_parser_flow() {
        //  mock p2pool.conf
        let base = temp_dir().join("pdm_p2pool_test");
        let _ = std::fs::remove_dir_all(&base);
        create_dir_all(&base).unwrap();
        let conf_path = base.join("p2pool.conf");

        std::fs::write(&conf_path, b"listen_port=9332\nwallet=abc").unwrap();

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.explorer.current_dir = base.clone();
        app.explorer.load_directory();

        let mut step = 0;
        let event_provider = |app: &mut App| {
            step += 1;
            match step {
                1 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                ))), // Bitcoin
                2 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                ))), // P2Pool
                3 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                ))), // Open explorer
                4 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::empty(),
                ))), // Move to p2pool.conf
                5 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::empty(),
                ))), // Select conf
                6 => Ok(Event::Key(KeyEvent::new(
                    KeyCode::Char('q'),
                    KeyModifiers::empty(),
                ))),

                _ => panic!("unexpected"),
            }
        };

        let res = run_app(&mut terminal, &mut app, event_provider);
        assert!(res.is_ok());

        assert_eq!(app.current_screen, CurrentScreen::P2PoolConfig);
        assert_eq!(app.p2pool_conf_path, Some(conf_path));
        assert!(
            !app.p2pool_editor.sections.is_empty(),
            "Editor should have loaded parsed sections"
        );
    }

    #[test]
    fn test_p2pool_editor_navigation() {
        let mut app = App::new();
        app.current_screen = CurrentScreen::P2PoolConfig;
        app.p2pool_conf_path = Some("dummy".into());

        app.p2pool_editor.load_data(vec![
            ConfigSection {
                title: "Network".into(),
                fields: vec![
                    ConfigField {
                        key: "port".into(),
                        value: "1".into(),
                    },
                    ConfigField {
                        key: "addr".into(),
                        value: "2".into(),
                    },
                ],
            },
            ConfigSection {
                title: "Payout".into(),
                fields: vec![ConfigField {
                    key: "wallet".into(),
                    value: "abc".into(),
                }],
            },
        ]);

        app.p2pool_editor.next_tab();
        assert_eq!(app.p2pool_editor.selected_tab, 1);

        app.p2pool_editor.previous_tab();
        assert_eq!(app.p2pool_editor.selected_tab, 0);

        app.p2pool_editor.next_field();
        assert_eq!(app.p2pool_editor.field_state.selected(), Some(1));
    }
}
