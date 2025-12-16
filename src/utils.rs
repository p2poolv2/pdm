// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

pub fn get_bitcoin_path() -> Option<PathBuf> {
    match std::env::consts::OS {
        "linux" => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".bitcoin/bitcoin.conf")),
        "macos" => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/Bitcoin/bitcoin.conf")),
        "windows" => {
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Bitcoin/bitcoin.conf"))
        }
        _ => None,
    }
}
