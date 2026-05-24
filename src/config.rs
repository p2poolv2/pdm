// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use config::{Config, File};

const DEFAULT_API_HOST: &str = "127.0.0.1";
const DEFAULT_API_PORT: u16 = 9332;

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub base_url: String,
    pub fallback_base_url: Option<String>,
    pub auth_user: Option<String>,
    pub auth_pass: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: format!("http://{}:{}", DEFAULT_API_HOST, DEFAULT_API_PORT),
            fallback_base_url: None,
            auth_user: None,
            auth_pass: None,
        }
    }
}

pub fn load_api_config() -> Result<ApiConfig> {
    let settings = Config::builder()
        .add_source(File::with_name("config/config").required(false))
        .add_source(
            File::with_name(concat!(env!("CARGO_MANIFEST_DIR"), "/config/config")).required(false),
        )
        .build()?;

    let host: String = settings
        .get("api.host")
        .unwrap_or_else(|_| DEFAULT_API_HOST.to_string());
    let port: u16 = settings.get("api.port").unwrap_or(DEFAULT_API_PORT);
    let base_url: String = settings
        .get("api.base_url")
        .unwrap_or_else(|_| format!("http://{}:{}", host, port));
    let fallback_base_url: Option<String> = settings
        .get("api.fallback_base_url")
        .ok()
        .filter(|url: &String| !url.trim().is_empty());
    let auth_user: Option<String> = settings.get("api.auth_user").ok();
    let auth_pass: Option<String> = settings.get("api.auth_pass").ok();

    Ok(ApiConfig {
        base_url,
        fallback_base_url,
        auth_user,
        auth_pass,
    })
}
