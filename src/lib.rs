// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod app;
pub mod components;
pub mod config;
pub mod settings;
pub mod systemd;
pub mod theme;
pub mod ui;

/// P2Poolv2 node release this build of pdm reads configs and API responses for.
///
/// Must match the `tag` of the `p2poolv2_config` dependency in Cargo.toml.
pub const P2POOLV2_VERSION: &str = "0.16.2";

#[cfg(test)]
mod tests {
    use super::P2POOLV2_VERSION;

    #[test]
    fn p2poolv2_version_matches_cargo_dependency_tag() {
        let cargo_toml = include_str!("../Cargo.toml");
        let expected_tag = format!("tag = \"v{P2POOLV2_VERSION}\"");
        assert!(
            cargo_toml.contains(&expected_tag),
            "Cargo.toml p2poolv2_config dependency must use {expected_tag}"
        );
    }
}
