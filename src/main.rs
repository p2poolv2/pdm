// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use p2poolv2_config::Config as P2PoolConfig;
use pdm::app::AppAction;
use pdm::app::{App, CurrentScreen};
use pdm::config::{parse_config as parse_bitcoin_config, write_config};
use pdm::ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::Backend, backend::CrosstermBackend};
use std::io;

fn main() -> Result<()> {
    // Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run App
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    // Restore Terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Hard exit (always allowed)
            if key.code == KeyCode::Char('q')
                || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))
            {
                return Ok(());
            }

            let action = match app.current_screen {
                CurrentScreen::FileExplorer => app.explorer.handle_input(key),

                CurrentScreen::BitcoinConfig if !app.bitcoin_data.is_empty() => {
                    app.bitcoin_config_view
                        .handle_input(key, &mut app.bitcoin_data)
                }

                CurrentScreen::BitcoinStatus => match key.code {
                    KeyCode::Left => {
                        if app.bitcoin_status_tab > 0 {
                            app.bitcoin_status_tab -= 1;
                        }
                        AppAction::None
                    }
                    KeyCode::Right => {
                        if app.bitcoin_status_tab < 3 {
                            app.bitcoin_status_tab += 1;
                        }
                        AppAction::None
                    }
                    KeyCode::Up => {
                        if app.sidebar_index > 0 {
                            app.sidebar_index -= 1;
                            AppAction::ToggleMenu
                        } else {
                            AppAction::None
                        }
                    }
                    KeyCode::Down => {
                        if app.sidebar_index < 3 {
                            app.sidebar_index += 1;
                            AppAction::ToggleMenu
                        } else {
                            AppAction::None
                        }
                    }
                    _ => AppAction::None,
                },

                _ => match key.code {
                    KeyCode::Enter => {
                        if matches!(
                            app.current_screen,
                            CurrentScreen::BitcoinConfig | CurrentScreen::P2PoolConfig
                        ) {
                            AppAction::OpenExplorer(app.current_screen.clone())
                        } else {
                            AppAction::None
                        }
                    }

                    KeyCode::Down => {
                        if app.sidebar_index < 7 {
                            app.sidebar_index += 1;
                            AppAction::ToggleMenu
                        } else {
                            AppAction::None
                        }
                    }

                    KeyCode::Up => {
                        if app.sidebar_index > 0 {
                            app.sidebar_index -= 1;
                            AppAction::ToggleMenu
                        } else {
                            AppAction::None
                        }
                    }

                    _ => AppAction::None,
                },
            };

            if handle_action(action, app)? {
                return Ok(());
            }
        }
    }
}

// Logic Handler
fn handle_action(action: AppAction, app: &mut App) -> Result<bool> {
    match action {
        AppAction::Quit => return Ok(true),

        AppAction::ToggleMenu => app.toggle_menu(),

        AppAction::OpenExplorer(trigger) => {
            app.explorer_trigger = Some(trigger);
            app.current_screen = CurrentScreen::FileExplorer;
        }

        AppAction::CloseModal => {
            if let Some(trigger) = &app.explorer_trigger {
                app.current_screen = trigger.clone();
            } else {
                app.toggle_menu();
            }
            app.explorer_trigger = None;
        }

        AppAction::FileSelected(path) => {
            if let Some(trigger) = &app.explorer_trigger {
                match trigger {
                    CurrentScreen::P2PoolConfig => {
                        app.p2pool_conf_path = Some(path.clone());
                        if let Ok(cfg) = P2PoolConfig::load(path.to_str().unwrap()) {
                            app.p2pool_config = Some(cfg);
                        }
                        app.current_screen = CurrentScreen::P2PoolConfig;
                    }
                    CurrentScreen::BitcoinConfig => {
                        app.bitcoin_conf_path = Some(path.clone());
                        if let Ok(entries) = parse_bitcoin_config(&path) {
                            app.bitcoin_data = entries;
                            app.bitcoin_config_view
                                .rebuild_sections(&app.bitcoin_data);
                        }
                        app.current_screen = CurrentScreen::BitcoinConfig;
                    }
                    _ => {}
                }
            }
            app.explorer_trigger = None;
        }

        AppAction::SaveConfig => {
            if let Some(ref path) = app.bitcoin_conf_path {
                if let Err(e) = write_config(path, &app.bitcoin_data) {
                    eprintln!("Failed to save config: {e}");
                }
            }
        }

        AppAction::Navigate(screen) => {
            app.current_screen = screen;
        }

        AppAction::None => {}
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_app_integration_smoke_test() {
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        // Initial render
        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!("home_screen", terminal.backend());

        // Simulate sidebar move
        app.sidebar_index = 1;
        app.toggle_menu();

        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
        insta::assert_debug_snapshot!("menu_toggled", terminal.backend());

        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);
    }

    #[test]
    fn test_file_explorer_flow_state_only() {
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        // Navigate to Bitcoin config
        app.sidebar_index = 1;
        app.toggle_menu();
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);

        // Open explorer
        handle_action(
            AppAction::OpenExplorer(CurrentScreen::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);

        // Close explorer
        handle_action(AppAction::CloseModal, &mut app).unwrap();
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);

        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
    }

    #[test]
    fn test_file_explorer_wrap_and_select_sets_config() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use std::fs::File;
        use tempfile::tempdir;

        // Create isolated temporary directory
        let dir = tempdir().unwrap();
        let base = dir.path();

        // Create a fake bitcoin.conf file
        let file_path = base.join("bitcoin.conf");
        File::create(&file_path).unwrap();

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        app.explorer.current_dir = base.to_path_buf();
        app.explorer.load_directory();

        handle_action(
            AppAction::OpenExplorer(CurrentScreen::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        // Move selection DOWN to the actual file (skip "..")
        app.explorer
            .handle_input(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));

        let action = app
            .explorer
            .handle_input(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        handle_action(action, &mut app).unwrap();

        assert_eq!(app.bitcoin_conf_path, Some(file_path));

        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
    }

    #[test]
    fn app_action_open_explorer_sets_state() {
        let mut app = App::new();

        let exited = handle_action(
            AppAction::OpenExplorer(CurrentScreen::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        assert!(!exited);
        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);
        assert_eq!(app.explorer_trigger, Some(CurrentScreen::BitcoinConfig));
    }

    #[test]
    fn app_action_close_modal_returns_to_trigger_screen() {
        let mut app = App::new();

        app.explorer_trigger = Some(CurrentScreen::BitcoinConfig);
        app.current_screen = CurrentScreen::FileExplorer;

        let exited = handle_action(AppAction::CloseModal, &mut app).unwrap();

        assert!(!exited);
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);
        assert!(app.explorer_trigger.is_none());
    }

    #[test]
    fn app_action_quit_requests_exit() {
        let mut app = App::new();

        let exited = handle_action(AppAction::Quit, &mut app).unwrap();

        assert!(exited);
    }
}
