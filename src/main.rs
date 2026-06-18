// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use p2poolv2_config::Config as P2PoolConfig;
use pdm::app::{
    App, AppAction, CurrentScreen, ExplorerTrigger, MAX_BITCOIN_STATUS_TAB, MAX_SIDEBAR_INDEX,
};
use pdm::bitcoin_config::{
    parse_config as parse_bitcoin_config, save_config as save_bitcoin_config,
};
use pdm::components::settings_view::{FIELDS, FieldKind};
use pdm::p2poolv2_config::{apply_edit as apply_p2pool_edit, flatten_config};
use pdm::settings::{load_settings, save_settings};
use pdm::ui;
use std::ops::ControlFlow;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::Backend, backend::CrosstermBackend};
use std::io;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run App
    let mut app = App::new();
    app.settings = load_settings();
    bootstrap_from_settings(&mut app);
    let res = run_app(&mut terminal, &mut app);

    // Restore Terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:#}");
    }

    Ok(())
}

fn sidebar_nav(key: KeyCode, app: &mut App) -> AppAction {
    match key {
        KeyCode::Up if app.sidebar_index > 0 => {
            app.sidebar_index -= 1;
            AppAction::ToggleMenu
        }
        KeyCode::Down if app.sidebar_index < MAX_SIDEBAR_INDEX => {
            app.sidebar_index += 1;
            AppAction::ToggleMenu
        }
        _ => AppAction::None,
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        app.poll_chain_info();
        terminal.draw(|f| ui::ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Ctrl-C is always a hard exit.
            // 'q' is suppressed while a text-input field is active.
            let text_input_active = (app.current_screen == CurrentScreen::BitcoinConfig
                && !app.bitcoin_config_view.sidebar_focused
                && app.bitcoin_config_view.editing)
                || (app.current_screen == CurrentScreen::P2PoolConfig
                    && !app.p2pool_config_view.sidebar_focused
                    && app.p2pool_config_view.editing);

            if (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))
                || (!text_input_active && key.code == KeyCode::Char('q'))
            {
                return Ok(());
            }

            let action = match app.current_screen {
                CurrentScreen::FileExplorer => app.explorer.handle_input(key),

                CurrentScreen::BitcoinStatus => match key.code {
                    KeyCode::Left => {
                        if app.bitcoin_status_tab > 0 {
                            app.bitcoin_status_tab -= 1;
                        }
                        AppAction::None
                    }
                    KeyCode::Right => {
                        if app.bitcoin_status_tab < MAX_BITCOIN_STATUS_TAB {
                            app.bitcoin_status_tab += 1;
                        }
                        AppAction::None
                    }
                    k => sidebar_nav(k, app),
                },

                CurrentScreen::BitcoinConfig => {
                    if app.bitcoin_conf_path.is_some() {
                        if app.bitcoin_config_view.sidebar_focused {
                            match key.code {
                                KeyCode::Enter => {
                                    app.bitcoin_config_view.sidebar_focused = false;
                                    AppAction::None
                                }
                                k => sidebar_nav(k, app),
                            }
                        } else {
                            let entries = &app.bitcoin_data;
                            app.bitcoin_config_view.handle_input(key, entries)
                        }
                    } else {
                        match key.code {
                            KeyCode::Enter => {
                                app.bitcoin_config_view.warning_message = None;
                                AppAction::OpenExplorer(ExplorerTrigger::BitcoinConfig)
                            }
                            KeyCode::Esc => AppAction::CloseModal,
                            k => sidebar_nav(k, app),
                        }
                    }
                }

                // P2Pool config
                CurrentScreen::P2PoolConfig => {
                    if app.p2pool_conf_path.is_some() {
                        if app.p2pool_config_view.sidebar_focused {
                            match key.code {
                                KeyCode::Enter => {
                                    app.p2pool_config_view.sidebar_focused = false;
                                    AppAction::None
                                }
                                k => sidebar_nav(k, app),
                            }
                        } else {
                            // Build flat entry list and delegate to the view
                            let entries = app
                                .p2pool_config
                                .as_ref()
                                .map(|cfg| flatten_config(cfg))
                                .unwrap_or_default();
                            app.p2pool_config_view.handle_input(key, &entries)
                        }
                    } else {
                        match key.code {
                            KeyCode::Enter => {
                                app.p2pool_config_view.warning_message = None;
                                AppAction::OpenExplorer(ExplorerTrigger::P2PoolConfig)
                            }
                            KeyCode::Esc => AppAction::CloseModal,
                            k => sidebar_nav(k, app),
                        }
                    }
                }

                CurrentScreen::Settings => {
                    if app.settings_view.sidebar_focused {
                        match key.code {
                            KeyCode::Enter => {
                                app.settings_view.sidebar_focused = false;
                                AppAction::None
                            }
                            k => sidebar_nav(k, app),
                        }
                    } else {
                        app.settings_view.handle_input(key)
                    }
                }

                _ => sidebar_nav(key.code, app),
            };

            if handle_action(action, app)?.is_break() {
                return Ok(());
            }
        }
    }
}

/// Pre-populate app state from `app.settings`. Called once at startup after
/// settings have been loaded into `app.settings = load_settings()`.
fn bootstrap_from_settings(app: &mut App) {
    // Bitcoin config
    if let Some(path) = &app.settings.bitcoin_conf_path {
        let entries = parse_bitcoin_config(path).unwrap_or_default();
        if entries.iter().any(|e| e.enabled && e.schema.is_some()) {
            app.bitcoin_conf_path = Some(path.clone());
            app.bitcoin_data = entries;
        }
    }

    // P2Pool config — only set the path when the config is actually loadable
    if let Some(path) = &app.settings.p2pool_conf_path.clone() {
        if let Some(p) = path.to_str() {
            match P2PoolConfig::load(p) {
                Ok(cfg) => {
                    app.p2pool_conf_path = Some(path.clone());
                    app.p2pool_config = Some(cfg);
                }
                Err(e) => {
                    eprintln!("pdm: failed to load p2pool config on startup: {e}");
                    // Leave both as None so the view prompts the user to re-select
                }
            }
        }
    }
}

// Logic Handler
#[allow(clippy::too_many_lines)] // Central dispatch; splitting would obscure the flow
fn handle_action(action: AppAction, app: &mut App) -> Result<ControlFlow<()>> {
    match action {
        AppAction::Quit => return Ok(ControlFlow::Break(())),

        AppAction::ToggleMenu => app.toggle_menu(),

        AppAction::OpenExplorer(trigger) => {
            if app.explorer.allow_dir_select {
                app.explorer.allow_dir_select = false;
                app.explorer.load_directory();
            }
            app.explorer_trigger = Some(trigger);
            app.current_screen = CurrentScreen::FileExplorer;
        }

        AppAction::OpenExplorerForSettings(field) => {
            let dir_select = FIELDS
                .get(field)
                .map_or(false, |f| matches!(f.1, FieldKind::DirectoryPicker));
            if app.explorer.allow_dir_select != dir_select {
                app.explorer.allow_dir_select = dir_select;
                app.explorer.load_directory();
            }
            app.explorer_trigger = Some(ExplorerTrigger::Settings(field));
            app.current_screen = CurrentScreen::FileExplorer;
        }

        AppAction::CloseModal => {
            app.explorer.allow_dir_select = false;
            app.explorer_trigger = None;
            app.toggle_menu();
        }

        AppAction::FileSelected(path) => {
            if let Some(trigger) = app.explorer_trigger.take() {
                match trigger {
                    ExplorerTrigger::P2PoolConfig => {
                        match P2PoolConfig::load(path.to_str().unwrap_or_default()) {
                            Ok(cfg) => {
                                // Sanity check — a valid p2pool config must have
                                // a stratum section with at least a hostname

                                if cfg.stratum.hostname.is_empty() {
                                    app.p2pool_config_view.warning_message = Some(
                                        "Config loaded but appears invalid: stratum.hostname is empty. Select another file."
                                            .to_string(),
                                    );
                                    app.p2pool_conf_path = None;
                                    app.p2pool_config = None;
                                } else {
                                    // Only set path + persist settings when config is actually valid
                                    app.p2pool_conf_path = Some(path.clone());
                                    app.p2pool_config = Some(cfg);
                                    app.p2pool_config_view.sidebar_focused = false;
                                    app.p2pool_config_view.warning_message = None;
                                    app.p2pool_config_view.selected_index = 0;
                                    app.settings.p2pool_conf_path = Some(path.clone());
                                    app.settings_view.save_error = None;
                                    if let Err(e) = save_settings(&app.settings) {
                                        app.settings_view.save_error =
                                            Some(format!("Save failed: {e}"));
                                    }
                                }
                            }
                            Err(e) => {
                                app.p2pool_config_view.warning_message = Some(format!(
                                    "Failed to load P2Pool config: {}. Select another file.",
                                    e
                                ));
                                app.p2pool_conf_path = None;
                                app.p2pool_config = None;
                            }
                        }
                        app.current_screen = CurrentScreen::P2PoolConfig;
                    }
                    ExplorerTrigger::BitcoinConfig => match parse_bitcoin_config(&path) {
                        Ok(entries) => {
                            const MIN_KNOWN_KEYS: usize = 1;
                            let known_key_count = entries
                                .iter()
                                .filter(|e| e.enabled && e.schema.is_some())
                                .count();

                            if known_key_count >= MIN_KNOWN_KEYS {
                                app.bitcoin_conf_path = Some(path.clone());
                                app.bitcoin_data = entries;
                                app.bitcoin_config_view.selected_index = 0;
                                app.bitcoin_config_view.dirty = false;
                                app.current_screen = CurrentScreen::BitcoinConfig;
                                app.bitcoin_config_view.sidebar_focused = false;
                                app.bitcoin_config_view.warning_message = None;
                                app.settings.bitcoin_conf_path = Some(path.clone());
                                app.settings_view.save_error = None;
                                if let Err(e) = save_settings(&app.settings) {
                                    let save_error = format!("Save failed: {e}");
                                    app.settings_view.save_error = Some(save_error.clone());
                                    app.bitcoin_config_view.warning_message = Some(save_error);
                                }
                            } else {
                                app.bitcoin_config_view.warning_message = Some(
                                    "File does not appear to be a Bitcoin config. Select another file."
                                        .to_string(),
                                );
                                app.current_screen = CurrentScreen::BitcoinConfig;
                            }
                        }
                        Err(e) => {
                            app.bitcoin_config_view.warning_message = Some(format!(
                                "Failed to read config: {e}. Check permissions and try again."
                            ));
                            app.current_screen = CurrentScreen::BitcoinConfig;
                        }
                    },
                    ExplorerTrigger::Settings(field) => {
                        app.explorer.allow_dir_select = false;
                        let mut should_save = true;
                        match field {
                            0 => match parse_bitcoin_config(&path) {
                                Ok(entries) => {
                                    let known_key_count = entries
                                        .iter()
                                        .filter(|e| e.enabled && e.schema.is_some())
                                        .count();
                                    if known_key_count >= 1 {
                                        app.bitcoin_conf_path = Some(path.clone());
                                        app.bitcoin_data = entries;
                                        app.bitcoin_config_view.selected_index = 0;
                                        app.bitcoin_config_view.dirty = false;
                                        app.bitcoin_config_view.warning_message = None;
                                        app.settings.bitcoin_conf_path = Some(path.clone());
                                    } else {
                                        app.settings_view.save_error = Some(
                                            "File does not appear to be a Bitcoin config."
                                                .to_string(),
                                        );
                                        should_save = false;
                                    }
                                }
                                Err(e) => {
                                    app.settings_view.save_error =
                                        Some(format!("Failed to read config: {e}"));
                                    should_save = false;
                                }
                            },
                            1 => match P2PoolConfig::load(path.to_str().unwrap_or_default()) {
                                Ok(cfg) => {
                                    if cfg.stratum.hostname.is_empty() {
                                        app.settings_view.save_error = Some(
                                            "Config appears invalid: stratum.hostname is empty."
                                                .to_string(),
                                        );
                                        should_save = false;
                                    } else {
                                        app.p2pool_config = Some(cfg);
                                        app.settings.p2pool_conf_path = Some(path.clone());
                                        app.p2pool_config_view.warning_message = None;
                                        app.p2pool_config_view.selected_index = 0;
                                        app.settings.p2pool_conf_path = Some(path.clone());
                                    }
                                }
                                Err(e) => {
                                    app.settings_view.save_error =
                                        Some(format!("Failed to load P2Pool config: {}", e));
                                    should_save = false;
                                }
                            },
                            2 => app.settings.ln_conf_path = Some(path.clone()),
                            3 => app.settings.shares_market_conf_path = Some(path.clone()),
                            4 => app.settings.settings_dir_override = Some(path.clone()),
                            _ => {}
                        }
                        if should_save {
                            app.settings_view.save_error = None;
                            if let Err(e) = save_settings(&app.settings) {
                                app.settings_view.save_error = Some(format!("Save failed: {e}"));
                            }
                        }
                        app.current_screen = CurrentScreen::Settings;
                        app.settings_view.sidebar_focused = false;
                    }
                }
            }
        }

        AppAction::SaveBitcoinConfig => {
            if let Some(path) = &app.bitcoin_conf_path {
                save_bitcoin_config(path, &app.bitcoin_data)?;
                app.bitcoin_config_view.save_message =
                    Some("Configuration correctly saved".to_string());
                app.bitcoin_config_view.dirty = false;
            }
        }

        AppAction::Navigate(screen) => {
            app.current_screen = screen;
        }

        AppAction::CommitEdit(index, value) => {
            if index < app.bitcoin_data.len() {
                app.bitcoin_data[index].value = value;
                app.bitcoin_data[index].enabled = true;
                app.bitcoin_config_view.dirty = true;
            }
        }

        AppAction::ClearSettingsField(field) => {
            match field {
                0 => {
                    app.settings.bitcoin_conf_path = None;
                    app.bitcoin_conf_path = None;
                    app.bitcoin_data.clear();
                }
                1 => {
                    app.settings.p2pool_conf_path = None;
                    app.p2pool_conf_path = None;
                    app.p2pool_config = None;
                }
                2 => app.settings.ln_conf_path = None,
                3 => app.settings.shares_market_conf_path = None,
                4 => app.settings.settings_dir_override = None,
                _ => {}
            }
            app.settings_view.save_error = None;
            if let Err(e) = save_settings(&app.settings) {
                app.settings_view.save_error = Some(format!("Save failed: {e}"));
            }
        }
        AppAction::CommitP2PoolEdit(index, value) => {
            if let Some(cfg) = app.p2pool_config.as_mut() {
                match apply_p2pool_edit(cfg, index, &value) {
                    Ok(()) => {
                        app.p2pool_config_view.warning_message = None;
                    }
                    Err(e) => {
                        app.p2pool_config_view.warning_message = Some(e);
                    }
                }
            }
        }

        AppAction::SaveP2PoolConfig => {
            if let (Some(path), Some(cfg)) =
                (app.p2pool_conf_path.clone(), app.p2pool_config.as_ref())
            {
                match save_p2pool_config(&path, cfg) {
                    Ok(()) => {
                        app.p2pool_config_view.save_message =
                            Some("Configuration correctly saved".to_string());
                    }
                    Err(e) => {
                        app.p2pool_config_view.warning_message =
                            Some(format!("Save failed: {}", e));
                    }
                }
            }
        }

        AppAction::None => {}
    }

    Ok(ControlFlow::Continue(()))
}

/// Matches the TOML type of an existing item and parses the new string
/// value into that same type. This prevents numeric/bool fields from
/// being written back as quoted strings (e.g. port = "3333").
fn typed_toml_item_like(existing: &toml_edit::Item, new_value: &str) -> Result<toml_edit::Item> {
    if existing.as_integer().is_some() {
        let parsed = new_value
            .parse::<i64>()
            .map_err(|e| anyhow::anyhow!("Expected integer, got '{}': {}", new_value, e))?;
        Ok(toml_edit::value(parsed))
    } else if existing.as_float().is_some() {
        let parsed = new_value
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("Expected float, got '{}': {}", new_value, e))?;
        Ok(toml_edit::value(parsed))
    } else if existing.as_bool().is_some() {
        let parsed = new_value.parse::<bool>().map_err(|e| {
            anyhow::anyhow!("Expected bool (true/false), got '{}': {}", new_value, e)
        })?;
        Ok(toml_edit::value(parsed))
    } else if existing.as_str().is_some() {
        Ok(toml_edit::value(new_value.to_owned()))
    } else if existing.as_array().is_some() {
        let values: Vec<String> = new_value
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let mut new_arr = toml_edit::Array::default();
        for value in values {
            new_arr.push(value);
        }
        Ok(toml_edit::Item::Value(toml_edit::Value::Array(new_arr)))
    } else {
        Err(anyhow::anyhow!(
            "Unsupported TOML value type for key: {}",
            existing
        ))
    }
}

/// Serialize the live `P2PoolConfig` back to TOML and write it to disk.
/// Saves P2Pool config by patching the original TOML file in-place.
/// Uses toml_edit so comments and formatting are preserved.
fn save_p2pool_config(path: &std::path::Path, cfg: &P2PoolConfig) -> Result<()> {
    use pdm::p2poolv2_config::flatten_config;
    use toml_edit::DocumentMut;

    // Read the original file so we preserve comments/ordering
    let original = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read P2Pool config: {}", e))?;

    let mut doc = original
        .parse::<DocumentMut>()
        .map_err(|e| anyhow::anyhow!("Failed to parse P2Pool config TOML: {}", e))?;

    // Walk every flattened entry and patch the matching TOML key
    for entry in flatten_config(cfg) {
        let section = entry.section.to_string();
        let key = entry.key.as_str();

        // Skip optional fields that are unset — leave them absent in the file
        if !entry.enabled {
            continue;
        }

        if let Some(table) = doc.get_mut(&section).and_then(|v| v.as_table_mut()) {
            // Only update keys that already exist in the file to avoid
            // injecting fields the user intentionally omitted
            if let Some(existing) = table.get(key) {
                match typed_toml_item_like(existing, &entry.value) {
                    Ok(updated) => table[key] = updated,
                    Err(e) => {
                        // Soft error — skip this field and continue
                        // saving the rest rather than aborting entirely
                        eprintln!("Warning: skipping {}.{}: {}", section, key, e);
                    }
                }
            }
        }
    }

    std::fs::write(path, doc.to_string())
        .map_err(|e| anyhow::anyhow!("Failed to write P2Pool config: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use serial_test::serial;

    /// Redirects `save_settings` (and `load_settings`) to `dir` by setting
    /// `PDM_CONFIG_DIR`. Must only be called from tests annotated with `#[serial]`
    /// so that no two tests mutate this env var concurrently.
    fn redirect_saves_to(dir: &tempfile::TempDir) {
        // SAFETY: This function is only called from #[serial] tests, which the
        // serial_test crate serialises within the process via a mutex. No concurrent
        // read or write of PDM_CONFIG_DIR can occur while the lock is held.
        unsafe { std::env::set_var("PDM_CONFIG_DIR", dir.path()) };
    }

    /// Run an action for its side effects, discarding the ControlFlow return.
    fn run(action: AppAction, app: &mut App) {
        let _ = handle_action(action, app).unwrap();
    }

    /// Write a  p2pool TOML to `path`.
    fn write_valid_p2pool_toml(path: &std::path::Path) {
        std::fs::write(
            path,
            r#"
[network]
listen_address = "/ip4/127.0.0.1/tcp/6884"
dial_peers = []
max_pending_incoming = 10
max_pending_outgoing = 10
max_established_incoming = 50
max_established_outgoing = 50
max_established_per_peer = 1
max_workbase_per_second = 10
max_userworkbase_per_second = 10
max_miningshare_per_second = 100
max_inventory_per_second = 100
max_transaction_per_second = 100
max_requests_per_second = 100
dial_timeout_secs = 30

[store]
path = "./store.db"
background_task_frequency_hours = 24
pplns_ttl_days = 7

[stratum]
hostname = "pool.example.com"
port = 3333
start_difficulty = 10000
minimum_difficulty = 100
solo_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
bootstrap_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
zmqpubhashblock = "tcp://127.0.0.1:28332"
network = "signet"
version_mask = "1fffe000"
difficulty_multiplier = 1.0
pool_signature = "P2Poolv2"

[bitcoinrpc]
url = "http://127.0.0.1:38332"
username = "p2pool"
password = "p2pool"

[logging]
file = "./logs/p2pool.log"
console = true
level = "info"
stats_dir = "./logs/stats"

[api]
hostname = "127.0.0.1"
port = 46884
"#,
        )
        .unwrap();
    }

    /// Write a TOML that parses fine but has an empty hostname (fails sanity check).
    fn write_empty_hostname_toml(path: &std::path::Path) {
        std::fs::write(
            path,
            r#"
[network]
listen_address = "/ip4/127.0.0.1/tcp/6884"
dial_peers = []
max_pending_incoming = 10
max_pending_outgoing = 10
max_established_incoming = 50
max_established_outgoing = 50
max_established_per_peer = 1
max_workbase_per_second = 10
max_userworkbase_per_second = 10
max_miningshare_per_second = 100
max_inventory_per_second = 100
max_transaction_per_second = 100
max_requests_per_second = 100
dial_timeout_secs = 30

[store]
path = "./store.db"
background_task_frequency_hours = 24
pplns_ttl_days = 7

[stratum]
hostname = ""   # empty hostname should trigger a warning
port = 3333
start_difficulty = 10000
minimum_difficulty = 100
solo_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
bootstrap_address = "tb1qyazxde6558qj6z3d9np5e6msmrspwpf6k0qggk"
zmqpubhashblock = "tcp://127.0.0.1:28332"
network = "signet"
version_mask = "1fffe000"
difficulty_multiplier = 1.0
pool_signature = "P2Poolv2"

[bitcoinrpc]
url = "http://127.0.0.1:38332"
username = "p2pool"
password = "p2pool"

[logging]
file = "./logs/p2pool.log"
console = true
level = "info"
stats_dir = "./logs/stats"

[api]
hostname = "127.0.0.1"
port = 46884
"#,
        )
        .unwrap();
    }

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
        let _ = handle_action(
            AppAction::OpenExplorer(ExplorerTrigger::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);

        // Close explorer
        let _ = handle_action(AppAction::CloseModal, &mut app).unwrap();
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);

        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
    }

    #[test]
    #[serial]
    fn test_file_explorer_wrap_and_select_sets_config() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use tempfile::tempdir;

        // Create isolated temporary directory
        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let base = dir.path();

        // Create a fake bitcoin.conf file
        let file_path = base.join("bitcoin.conf");
        std::fs::write(&file_path, "rpcuser=test\n").unwrap();

        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        app.explorer.current_dir = base.to_path_buf();
        app.explorer.load_directory();

        let _ = handle_action(
            AppAction::OpenExplorer(ExplorerTrigger::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        // Move selection DOWN to the actual file (skip "..")
        app.explorer
            .handle_input(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));

        let action = app
            .explorer
            .handle_input(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        let _ = handle_action(action, &mut app).unwrap();

        assert_eq!(app.bitcoin_conf_path, Some(file_path));

        terminal.draw(|f| ui::ui(f, &mut app)).unwrap();
    }

    #[test]
    fn app_action_open_explorer_sets_state() {
        let mut app = App::new();

        let flow = handle_action(
            AppAction::OpenExplorer(ExplorerTrigger::BitcoinConfig),
            &mut app,
        )
        .unwrap();

        assert!(flow.is_continue());
        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);
        assert_eq!(app.explorer_trigger, Some(ExplorerTrigger::BitcoinConfig));
    }

    #[test]
    fn app_action_close_modal_returns_to_sidebar() {
        let mut app = App::new();

        app.sidebar_index = 1; // Bitcoin Config
        app.explorer_trigger = Some(ExplorerTrigger::BitcoinConfig);
        app.current_screen = CurrentScreen::FileExplorer;

        let flow = handle_action(AppAction::CloseModal, &mut app).unwrap();

        assert!(flow.is_continue());
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);
        assert!(app.explorer_trigger.is_none());
    }

    #[test]
    fn app_action_quit_requests_exit() {
        let mut app = App::new();

        let flow = handle_action(AppAction::Quit, &mut app).unwrap();

        assert!(flow.is_break());
    }

    #[test]
    fn commit_edit_updates_entry_value_and_enables_it() {
        use pdm::bitcoin_config::ConfigEntry;

        let mut app = App::new();
        app.bitcoin_data = vec![
            ConfigEntry {
                key: "rpcuser".to_string(),
                value: "old".to_string(),
                enabled: false,
                schema: None,
                section: None,
            },
            ConfigEntry {
                key: "server".to_string(),
                value: "0".to_string(),
                enabled: true,
                schema: None,
                section: None,
            },
        ];

        run(AppAction::CommitEdit(0, "alice".to_string()), &mut app);

        assert_eq!(app.bitcoin_data[0].value, "alice");
        assert!(app.bitcoin_data[0].enabled);
        // Other entries unchanged
        assert_eq!(app.bitcoin_data[1].value, "0");
    }

    #[test]
    fn commit_edit_out_of_bounds_is_noop() {
        let mut app = App::new();
        // bitcoin_data is empty
        let result = handle_action(AppAction::CommitEdit(5, "val".to_string()), &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn save_bitcoin_config_writes_file_and_sets_message() {
        use pdm::bitcoin_config::ConfigEntry;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bitcoin.conf");

        let mut app = App::new();
        app.bitcoin_conf_path = Some(path.clone());
        app.bitcoin_data = vec![ConfigEntry {
            key: "rpcuser".to_string(),
            value: "testuser".to_string(),
            enabled: true,
            schema: None,
            section: None,
        }];

        run(AppAction::SaveBitcoinConfig, &mut app);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("rpcuser=testuser"));
        assert_eq!(
            app.bitcoin_config_view.save_message.as_deref(),
            Some("Configuration correctly saved")
        );
    }

    #[test]
    fn save_bitcoin_config_noop_when_no_path() {
        let mut app = App::new();
        // No bitcoin_conf_path set
        let result = handle_action(AppAction::SaveBitcoinConfig, &mut app);
        assert!(result.is_ok());
        assert!(app.bitcoin_config_view.save_message.is_none());
    }

    #[test]
    fn navigate_action_changes_screen() {
        let mut app = App::new();
        run(AppAction::Navigate(CurrentScreen::BitcoinStatus), &mut app);
        assert_eq!(app.current_screen, CurrentScreen::BitcoinStatus);
    }

    #[test]
    fn file_selected_invalid_bitcoin_config_sets_warning() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("not_a_config.conf");
        // Write a file with no recognized bitcoin config keys
        std::fs::write(&path, "unknownkey=somevalue\n").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::BitcoinConfig);

        run(AppAction::FileSelected(path), &mut app);

        assert!(app.bitcoin_config_view.warning_message.is_some());
        assert!(app.bitcoin_conf_path.is_none());
        assert_eq!(app.current_screen, CurrentScreen::BitcoinConfig);
    }

    #[test]
    #[serial]
    fn bitcoin_config_sidebar_focus_toggle_via_enter() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("bitcoin.conf");
        std::fs::write(&path, "rpcuser=test\n").unwrap();

        let mut app = App::new();
        app.sidebar_index = 1;
        app.toggle_menu();
        run(
            AppAction::OpenExplorer(ExplorerTrigger::BitcoinConfig),
            &mut app,
        );

        app.explorer.current_dir = dir.path().to_path_buf();
        app.explorer.load_directory();

        // Select the file
        app.explorer
            .handle_input(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));
        let action = app
            .explorer
            .handle_input(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
        run(action, &mut app);

        // After file selection, sidebar_focused should be false
        assert!(!app.bitcoin_config_view.sidebar_focused);

        // Pressing Esc via handle_input should set sidebar_focused back
        let entries_clone = app.bitcoin_data.clone();
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        app.bitcoin_config_view.handle_input(esc, &entries_clone);
        assert!(app.bitcoin_config_view.sidebar_focused);
    }

    // toggle_menu state cleanup

    #[test]
    fn toggle_menu_clears_bitcoin_config_messages_on_navigate_away() {
        let mut app = App::new();
        app.sidebar_index = 1;
        app.toggle_menu(); // → BitcoinConfig
        app.bitcoin_config_view.warning_message = Some("some warning".to_string());
        app.bitcoin_config_view.save_message = Some("saved".to_string());

        app.sidebar_index = 0;
        app.toggle_menu(); // → Home

        assert!(app.bitcoin_config_view.warning_message.is_none());
        assert!(app.bitcoin_config_view.save_message.is_none());
    }

    #[test]
    fn toggle_menu_cancels_in_progress_edit_on_navigate_away() {
        let mut app = App::new();
        app.sidebar_index = 1;
        app.toggle_menu();
        app.bitcoin_config_view.editing = true;
        app.bitcoin_config_view.edit_input = "draft value".to_string();

        app.sidebar_index = 0;
        app.toggle_menu(); // navigate away

        assert!(!app.bitcoin_config_view.editing);
        assert!(app.bitcoin_config_view.edit_input.is_empty());
    }

    #[test]
    fn toggle_menu_does_not_clear_messages_when_staying_on_other_screen() {
        let mut app = App::new();
        // Start on Home (index 0), set some other state, navigate within Home
        app.sidebar_index = 2;
        app.toggle_menu(); // → BitcoinStatus
        app.bitcoin_config_view.warning_message = Some("keep me".to_string());

        app.sidebar_index = 3;
        app.toggle_menu(); // → P2PoolConfig (never on BitcoinConfig, no clear should happen)

        // Messages only cleared when leaving BitcoinConfig, not from other screens
        assert_eq!(
            app.bitcoin_config_view.warning_message.as_deref(),
            Some("keep me")
        );
    }

    // dirty flag

    #[test]
    fn commit_edit_sets_dirty_flag() {
        use pdm::bitcoin_config::ConfigEntry;

        let mut app = App::new();
        app.bitcoin_data = vec![ConfigEntry {
            key: "rpcuser".to_string(),
            value: "old".to_string(),
            enabled: true,
            schema: None,
            section: None,
        }];

        run(AppAction::CommitEdit(0, "new".to_string()), &mut app);

        assert!(app.bitcoin_config_view.dirty);
        assert_eq!(app.bitcoin_data[0].value, "new");
    }

    #[test]
    fn save_bitcoin_config_clears_dirty_flag() {
        use pdm::bitcoin_config::ConfigEntry;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bitcoin.conf");

        let mut app = App::new();
        app.bitcoin_conf_path = Some(path.clone());
        app.bitcoin_config_view.dirty = true;
        app.bitcoin_data = vec![ConfigEntry {
            key: "rpcuser".to_string(),
            value: "testuser".to_string(),
            enabled: true,
            schema: None,
            section: None,
        }];

        run(AppAction::SaveBitcoinConfig, &mut app);

        assert!(!app.bitcoin_config_view.dirty);
    }

    #[test]
    #[serial]
    fn file_selected_resets_dirty_flag() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("bitcoin.conf");
        std::fs::write(&path, "rpcuser=test\n").unwrap();

        let mut app = App::new();
        app.bitcoin_config_view.dirty = true;
        app.explorer_trigger = Some(ExplorerTrigger::BitcoinConfig);

        run(AppAction::FileSelected(path), &mut app);

        assert!(!app.bitcoin_config_view.dirty);
    }

    #[test]
    fn commit_edit_out_of_bounds_does_not_set_dirty() {
        let mut app = App::new();
        // bitcoin_data is empty; CommitEdit with bad index must not set dirty
        run(AppAction::CommitEdit(99, "val".to_string()), &mut app);
        assert!(!app.bitcoin_config_view.dirty);
    }

    // --- Settings handle_action tests ---

    #[test]
    fn open_explorer_for_settings_sets_state() {
        let mut app = App::new();
        app.sidebar_index = 8;
        app.toggle_menu();

        let flow = handle_action(AppAction::OpenExplorerForSettings(1), &mut app).unwrap();

        assert!(flow.is_continue());
        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);
        assert_eq!(app.explorer_trigger, Some(ExplorerTrigger::Settings(1)));
    }

    #[test]
    #[serial]
    fn file_selected_for_settings_stores_path_and_saves() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("ln.conf");
        std::fs::write(&path, "").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(2)); // ln_conf_path

        run(AppAction::FileSelected(path.clone()), &mut app);

        assert_eq!(app.settings.ln_conf_path, Some(path));
        assert_eq!(app.current_screen, CurrentScreen::Settings);
        assert!(!app.settings_view.sidebar_focused);
    }

    #[test]
    #[serial]
    fn file_selected_bitcoin_config_persists_to_settings() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("bitcoin.conf");
        std::fs::write(&path, "rpcuser=test\n").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::BitcoinConfig);

        run(AppAction::FileSelected(path.clone()), &mut app);

        assert_eq!(app.settings.bitcoin_conf_path, Some(path));
    }

    #[test]
    fn bootstrap_from_settings_loads_bitcoin_config() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bitcoin.conf");
        std::fs::write(&path, "rpcuser=test\n").unwrap();

        let mut app = App::new();
        app.settings.bitcoin_conf_path = Some(path.clone());

        bootstrap_from_settings(&mut app);

        assert_eq!(app.bitcoin_conf_path, Some(path));
        assert!(!app.bitcoin_data.is_empty());
    }

    #[test]
    fn bootstrap_from_settings_ignores_invalid_bitcoin_config() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.conf");
        std::fs::write(&path, "notakey=value\n").unwrap();

        let mut app = App::new();
        app.settings.bitcoin_conf_path = Some(path);

        bootstrap_from_settings(&mut app);

        // Invalid config: bitcoin_conf_path must NOT be set on app
        assert!(app.bitcoin_conf_path.is_none());
    }

    // Fix 13: Settings sidebar keyboard handler respects MAX_SIDEBAR_INDEX
    #[test]
    fn settings_sidebar_down_nav_respects_max_sidebar_index() {
        let mut app = App::new();
        // Navigate to Settings (last item, index MAX_SIDEBAR_INDEX)
        app.sidebar_index = MAX_SIDEBAR_INDEX;
        app.toggle_menu();
        assert_eq!(app.current_screen, CurrentScreen::Settings);
        assert!(app.settings_view.sidebar_focused);

        // Down at the last item must not go past MAX_SIDEBAR_INDEX
        let action = sidebar_nav(KeyCode::Down, &mut app);
        assert!(matches!(action, AppAction::None));
        assert_eq!(app.sidebar_index, MAX_SIDEBAR_INDEX);
    }

    #[test]
    fn settings_sidebar_up_nav_moves_to_previous_item() {
        let mut app = App::new();
        app.sidebar_index = MAX_SIDEBAR_INDEX;
        app.toggle_menu();

        let action = sidebar_nav(KeyCode::Up, &mut app);
        assert!(matches!(action, AppAction::ToggleMenu));
        assert_eq!(app.sidebar_index, MAX_SIDEBAR_INDEX - 1);
    }

    // Fix 14: bootstrap_from_settings with a valid P2Pool config path
    #[test]
    fn bootstrap_from_settings_loads_p2pool_conf_path() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("p2pool.toml");
        // Write a minimal but syntactically valid TOML file; P2PoolConfig::load
        // may fail to parse it, but bootstrap_from_settings should at least set
        // app.p2pool_conf_path regardless of whether the config is parseable.
        let cfg = write_valid_p2pool_toml(&path);

        let mut app = App::new();
        app.settings.p2pool_conf_path = Some(path.clone());

        bootstrap_from_settings(&mut app);

        // The path must always be set, even if the config fails to parse.
        assert_eq!(app.p2pool_conf_path, Some(path));
    }

    // Fix 15: file_selected_for_settings for fields 0, 1, 3 and the wildcard arm
    #[test]
    #[serial]
    fn file_selected_for_settings_field_0_bitcoin_conf_path() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("bitcoin.conf");
        std::fs::write(&path, "rpcuser=test\n").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(0));
        run(AppAction::FileSelected(path.clone()), &mut app);

        assert_eq!(app.settings.bitcoin_conf_path, Some(path.clone()));
        assert_eq!(app.bitcoin_conf_path, Some(path));
        assert!(!app.bitcoin_data.is_empty());
        assert_eq!(app.current_screen, CurrentScreen::Settings);
    }

    #[test]
    #[serial]
    fn file_selected_for_settings_field_1_p2pool_conf_path() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("p2pool.toml");
        let cfg = write_valid_p2pool_toml(&path);

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(1));
        run(AppAction::FileSelected(path.clone()), &mut app);

        assert_eq!(app.settings.p2pool_conf_path, Some(path));
        assert_eq!(app.current_screen, CurrentScreen::Settings);
    }

    #[test]
    #[serial]
    fn file_selected_for_settings_field_3_shares_market_conf_path() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("shares.conf");
        std::fs::write(&path, "").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(3));
        run(AppAction::FileSelected(path.clone()), &mut app);

        assert_eq!(app.settings.shares_market_conf_path, Some(path));
        assert_eq!(app.current_screen, CurrentScreen::Settings);
    }

    #[test]
    #[serial]
    fn file_selected_for_settings_wildcard_field_is_noop() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        let path = dir.path().join("unknown.conf");
        std::fs::write(&path, "").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(99));
        run(AppAction::FileSelected(path), &mut app);

        // None of the settings fields must have been touched
        assert!(app.settings.bitcoin_conf_path.is_none());
        assert!(app.settings.p2pool_conf_path.is_none());
        assert!(app.settings.ln_conf_path.is_none());
        assert!(app.settings.shares_market_conf_path.is_none());
        assert!(app.settings.settings_dir_override.is_none());
        assert_eq!(app.current_screen, CurrentScreen::Settings);
    }

    #[test]
    #[serial]
    fn file_selected_for_settings_field_4_sets_dir_override() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);
        // The "path" returned by the sentinel is the directory itself.
        let settings_dir = tempdir().unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::Settings(4));
        run(
            AppAction::FileSelected(settings_dir.path().to_path_buf()),
            &mut app,
        );

        assert_eq!(
            app.settings.settings_dir_override,
            Some(settings_dir.path().to_path_buf())
        );
        assert_eq!(app.current_screen, CurrentScreen::Settings);
        assert!(!app.settings_view.sidebar_focused);
        // allow_dir_select must be reset after selection
        assert!(!app.explorer.allow_dir_select);
    }

    #[test]
    fn open_explorer_for_settings_field4_enables_dir_select() {
        let mut app = App::new();
        run(AppAction::OpenExplorerForSettings(4), &mut app);
        assert!(app.explorer.allow_dir_select);
        assert_eq!(app.current_screen, CurrentScreen::FileExplorer);
    }

    #[test]
    fn open_explorer_for_settings_non_dir_field_disables_dir_select() {
        let mut app = App::new();
        // First enable dir select, then open a file-picker field — must reset.
        app.explorer.allow_dir_select = true;
        run(AppAction::OpenExplorerForSettings(0), &mut app);
        assert!(!app.explorer.allow_dir_select);
    }

    #[test]
    fn close_modal_resets_allow_dir_select() {
        let mut app = App::new();
        app.explorer.allow_dir_select = true;
        app.explorer_trigger = Some(ExplorerTrigger::Settings(4));
        app.current_screen = CurrentScreen::FileExplorer;
        app.sidebar_index = MAX_SIDEBAR_INDEX;

        run(AppAction::CloseModal, &mut app);

        assert!(!app.explorer.allow_dir_select);
        assert!(app.explorer_trigger.is_none());
    }

    // Fix 16: CloseModal clears the ExplorerTrigger when triggered from Settings
    #[test]
    fn close_modal_clears_settings_explorer_trigger() {
        let mut app = App::new();
        app.sidebar_index = MAX_SIDEBAR_INDEX; // Settings
        app.explorer_trigger = Some(ExplorerTrigger::Settings(2));
        app.current_screen = CurrentScreen::FileExplorer;

        run(AppAction::CloseModal, &mut app);

        assert!(app.explorer_trigger.is_none());
        assert_eq!(app.current_screen, CurrentScreen::Settings);
    }

    // --- ClearSettingsField ---

    #[test]
    #[serial]
    fn clear_settings_field_removes_path_and_saves() {
        use std::path::PathBuf;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);

        let mut app = App::new();
        app.settings.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));
        app.settings.p2pool_conf_path = Some(PathBuf::from("/tmp/p2pool.toml"));
        app.settings.ln_conf_path = Some(PathBuf::from("/tmp/ln.conf"));
        app.settings.shares_market_conf_path = Some(PathBuf::from("/tmp/shares.conf"));
        app.bitcoin_conf_path = Some(PathBuf::from("/tmp/bitcoin.conf"));
        app.p2pool_conf_path = Some(PathBuf::from("/tmp/p2pool.toml"));

        run(AppAction::ClearSettingsField(0), &mut app);
        assert!(app.settings.bitcoin_conf_path.is_none());
        assert!(app.bitcoin_conf_path.is_none());
        assert!(app.bitcoin_data.is_empty());

        run(AppAction::ClearSettingsField(1), &mut app);
        assert!(app.settings.p2pool_conf_path.is_none());
        assert!(app.p2pool_conf_path.is_none());
        assert!(app.p2pool_config.is_none());

        run(AppAction::ClearSettingsField(2), &mut app);
        assert!(app.settings.ln_conf_path.is_none());

        run(AppAction::ClearSettingsField(3), &mut app);
        assert!(app.settings.shares_market_conf_path.is_none());
    }

    #[test]
    #[serial]
    fn clear_settings_field_out_of_bounds_is_noop() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);

        let mut app = App::new();
        // No settings are set; clearing a non-existent index must not panic
        let result = handle_action(AppAction::ClearSettingsField(99), &mut app);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn clear_settings_field_resets_save_error() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        redirect_saves_to(&dir);

        let mut app = App::new();
        app.settings_view.save_error = Some("previous error".to_string());

        run(AppAction::ClearSettingsField(0), &mut app);

        // A successful save clears the error
        assert!(app.settings_view.save_error.is_none());
    }

    #[test]
    fn commit_p2pool_edit_success_clears_warning() {
        use std::path::PathBuf;

        let mut app = App::new();
        let path = PathBuf::from("dummy.toml");
        write_valid_p2pool_toml(&path);

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p2pool.toml");
        write_valid_p2pool_toml(&file);

        let cfg = P2PoolConfig::load(file.to_str().unwrap()).unwrap();
        app.p2pool_config = Some(cfg);
        app.p2pool_config_view.warning_message = Some("old warning".to_string());

        run(
            AppAction::CommitP2PoolEdit(0, "/ip4/127.0.0.1/tcp/9999".to_string()),
            &mut app,
        );

        assert!(app.p2pool_config_view.warning_message.is_none());
    }

    #[test]
    fn commit_p2pool_edit_failure_sets_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p2pool.toml");
        write_valid_p2pool_toml(&file);

        let mut app = App::new();
        let cfg = P2PoolConfig::load(file.to_str().unwrap()).unwrap();
        app.p2pool_config = Some(cfg);

        // invalid numeric/bool/etc depending on index used by your flatten_config
        run(
            AppAction::CommitP2PoolEdit(9999, "bad-value".to_string()),
            &mut app,
        );

        assert!(app.p2pool_config_view.warning_message.is_some());
    }

    #[test]
    fn save_p2pool_config_action_success_sets_message() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p2pool.toml");
        write_valid_p2pool_toml(&file);

        let mut app = App::new();
        let cfg = P2PoolConfig::load(file.to_str().unwrap()).unwrap();

        app.p2pool_conf_path = Some(file.clone());
        app.p2pool_config = Some(cfg);

        run(AppAction::SaveP2PoolConfig, &mut app);

        assert_eq!(
            app.p2pool_config_view.save_message.as_deref(),
            Some("Configuration correctly saved")
        );
    }

    #[test]
    fn save_p2pool_config_action_failure_sets_warning() {
        let mut app = App::new();

        let bad_path = std::path::PathBuf::from("/definitely/missing/path.toml");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p2pool.toml");
        write_valid_p2pool_toml(&file);
        let cfg = P2PoolConfig::load(file.to_str().unwrap()).unwrap();

        app.p2pool_conf_path = Some(bad_path);
        app.p2pool_config = Some(cfg);

        run(AppAction::SaveP2PoolConfig, &mut app);

        assert!(app.p2pool_config_view.warning_message.is_some());
    }

    #[test]
    fn typed_toml_item_like_integer_success() {
        let existing = toml_edit::value(3333);
        let updated = typed_toml_item_like(&existing, "4444").unwrap();
        assert_eq!(updated.as_integer(), Some(4444));
    }

    #[test]
    fn typed_toml_item_like_float_success() {
        let existing = toml_edit::value(1.5);
        let updated = typed_toml_item_like(&existing, "2.5").unwrap();
        assert_eq!(updated.as_float(), Some(2.5));
    }

    #[test]
    fn typed_toml_item_like_bool_success() {
        let existing = toml_edit::value(true);
        let updated = typed_toml_item_like(&existing, "false").unwrap();
        assert_eq!(updated.as_bool(), Some(false));
    }

    #[test]
    fn typed_toml_item_like_string_success() {
        let existing = toml_edit::value("old");
        let updated = typed_toml_item_like(&existing, "new").unwrap();
        assert_eq!(updated.as_str(), Some("new"));
    }

    #[test]
    fn typed_toml_item_like_invalid_parse_fails() {
        let existing = toml_edit::value(true);
        assert!(typed_toml_item_like(&existing, "not_bool").is_err());

        let existing = toml_edit::value(1);
        assert!(typed_toml_item_like(&existing, "abc").is_err());
    }

    #[test]
    fn save_p2pool_config_missing_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let valid = dir.path().join("valid.toml");
        write_valid_p2pool_toml(&valid);
        let cfg = P2PoolConfig::load(valid.to_str().unwrap()).unwrap();

        let missing = dir.path().join("missing.toml");
        let result = save_p2pool_config(&missing, &cfg);
        assert!(result.is_err());
    }

    #[test]
    fn save_p2pool_config_invalid_toml_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bad.toml");
        std::fs::write(&file, "not valid toml = = =").unwrap();

        let valid = dir.path().join("valid.toml");
        write_valid_p2pool_toml(&valid);
        let cfg = P2PoolConfig::load(valid.to_str().unwrap()).unwrap();

        let result = save_p2pool_config(&file, &cfg);
        assert!(result.is_err());
    }

    #[test]
    fn file_selected_p2pool_invalid_hostname_sets_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p2pool.toml");
        write_empty_hostname_toml(&file);

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::P2PoolConfig);

        run(AppAction::FileSelected(file), &mut app);

        assert!(app.p2pool_config_view.warning_message.is_some());
        assert!(app.p2pool_conf_path.is_none());
        assert!(app.p2pool_config.is_none());
    }

    #[test]
    fn file_selected_p2pool_parse_failure_sets_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bad.toml");
        std::fs::write(&file, "invalid === toml").unwrap();

        let mut app = App::new();
        app.explorer_trigger = Some(ExplorerTrigger::P2PoolConfig);

        run(AppAction::FileSelected(file), &mut app);

        assert!(app.p2pool_config_view.warning_message.is_some());
        assert!(app.p2pool_conf_path.is_none());
    }

    #[test]
    fn bootstrap_from_settings_invalid_p2pool_keeps_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bad.toml");
        std::fs::write(&file, "invalid === toml").unwrap();

        let mut app = App::new();
        app.settings.p2pool_conf_path = Some(file);

        bootstrap_from_settings(&mut app);

        assert!(app.p2pool_conf_path.is_none());
        assert!(app.p2pool_config.is_none());
    }
}
