// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Semantic styles shared by all views.
//!
//! The styles are chosen to stay readable on both light and dark terminal
//! themes without detecting the terminal background:
//!
//! - No fixed background colors; the terminal's own background is kept.
//! - No white, black, gray or dark gray foregrounds. Their contrast
//!   depends entirely on the theme, and dark gray is often the same as the
//!   background in dark themes.
//! - Secondary text uses the DIM modifier on the default foreground.
//! - Selection uses REVERSED, which swaps the terminal's own foreground
//!   and background, so it always contrasts.
//! - Accent colors are limited to the standard ANSI red, green, yellow,
//!   cyan and magenta, which themes tune to be readable.

use ratatui::style::{Color, Modifier, Style};

/// Secondary text: placeholders, hints, loading messages, unfocused borders.
pub const MUTED: Style = Style::new().add_modifier(Modifier::DIM);

/// Primary values and headers that should stand out from labels.
pub const EMPHASIS: Style = Style::new().add_modifier(Modifier::BOLD);

/// Selected row or tab in lists, tables and tab bars.
pub const HIGHLIGHT: Style = Style::new().add_modifier(Modifier::REVERSED);

/// Key name in the status bar hints.
pub const KEY_HINT: Style = Style::new()
    .add_modifier(Modifier::REVERSED)
    .add_modifier(Modifier::BOLD);

/// Identifiers such as config keys, block hashes and miner addresses.
pub const ACCENT: Style = Style::new().fg(Color::Cyan);

/// Config section names.
pub const SECTION: Style = Style::new().fg(Color::Magenta);

/// Text being typed in an edit field.
pub const EDITING: Style = Style::new().fg(Color::Yellow);

/// Errors and failed states.
pub const ERROR: Style = Style::new().fg(Color::Red);

/// Successful operations and healthy states.
pub const SUCCESS: Style = Style::new().fg(Color::Green);

/// Warnings and degraded states.
pub const WARNING: Style = Style::new().fg(Color::Yellow);
