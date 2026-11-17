// SPDX-FileCopyrightText: 2024 PDM Authors
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const POOL_DASHBOARD: &str = include_str!("../assets/grafana/pool.json");
const USERS_DASHBOARD: &str = include_str!("../assets/grafana/users.json");

pub const DEFAULT_PROMETHEUS_URL: &str = "http://127.0.0.1:9090";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrafanaProvisioningPaths {
    pub datasource: PathBuf,
    pub dashboard_provider: PathBuf,
    pub dashboards: Vec<PathBuf>,
}

#[must_use]
pub fn generate_grafana_datasource_yaml(prometheus_url: &str) -> String {
    format!(
        "apiVersion: 1\n\ndatasources:\n  - name: Prometheus\n    type: prometheus\n    access: proxy\n    url: {}\n    isDefault: true\n    editable: true\n    jsonData:\n      httpMethod: POST\n      timeInterval: 5s\n",
        yaml_quote(prometheus_url)
    )
}

#[must_use]
pub fn generate_grafana_dashboard_provider_yaml(dashboards_dir: &Path) -> String {
    format!(
        "apiVersion: 1\n\nproviders:\n  - name: 'P2Poolv2'\n    orgId: 1\n    folder: ''\n    type: file\n    disableDeletion: true\n    updateIntervalSeconds: 10\n    allowUiUpdates: false\n    options:\n      path: {}\n",
        yaml_quote(&dashboards_dir.to_string_lossy())
    )
}

/// Writes Grafana datasource provisioning, dashboard provider configuration,
/// and the bundled public P2Pool dashboards.
pub fn write_grafana_provisioning(prometheus_url: &str) -> Result<GrafanaProvisioningPaths> {
    let root = crate::prometheus::monitoring_config_dir()?.join("grafana");
    write_grafana_provisioning_in(&root, prometheus_url)
}

fn write_grafana_provisioning_in(
    root: &Path,
    prometheus_url: &str,
) -> Result<GrafanaProvisioningPaths> {
    let datasource_dir = root.join("provisioning/datasources");
    let provider_dir = root.join("provisioning/dashboards");
    let dashboards_dir = root.join("dashboards");

    for dir in [&datasource_dir, &provider_dir, &dashboards_dir] {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create Grafana directory: {}", dir.display()))?;
    }

    let datasource = datasource_dir.join("prometheus.yml");
    std::fs::write(
        &datasource,
        generate_grafana_datasource_yaml(prometheus_url),
    )
    .with_context(|| {
        format!(
            "could not write Grafana datasource: {}",
            datasource.display()
        )
    })?;

    let dashboard_provider = provider_dir.join("dashboards.yml");
    std::fs::write(
        &dashboard_provider,
        generate_grafana_dashboard_provider_yaml(&dashboards_dir),
    )
    .with_context(|| {
        format!(
            "could not write Grafana dashboard provider: {}",
            dashboard_provider.display()
        )
    })?;

    let pool = dashboards_dir.join("pool.json");
    std::fs::write(&pool, POOL_DASHBOARD)
        .with_context(|| format!("could not write Pool dashboard: {}", pool.display()))?;

    let users = dashboards_dir.join("users.json");
    std::fs::write(&users, USERS_DASHBOARD)
        .with_context(|| format!("could not write Users dashboard: {}", users.display()))?;

    Ok(GrafanaProvisioningPaths {
        datasource,
        dashboard_provider,
        dashboards: vec![pool, users],
    })
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn datasource_uses_configured_prometheus_url_as_default() {
        let yaml = generate_grafana_datasource_yaml("http://192.0.2.4:9191");

        assert!(yaml.contains("name: Prometheus"));
        assert!(yaml.contains("type: prometheus"));
        assert!(yaml.contains("url: 'http://192.0.2.4:9191'"));
        assert!(yaml.contains("isDefault: true"));
        assert!(!yaml.contains("password"));
    }

    #[test]
    fn datasource_escapes_single_quotes() {
        let yaml = generate_grafana_datasource_yaml("http://example.com/a'b");

        assert!(yaml.contains("url: 'http://example.com/a''b'"));
    }

    #[test]
    fn dashboard_provider_uses_generated_assets_directory() {
        let path = Path::new("/home/example/.config/pdm/monitoring/grafana/dashboards");
        let yaml = generate_grafana_dashboard_provider_yaml(path);

        assert!(yaml.contains("name: 'P2Poolv2'"));
        assert!(yaml.contains("type: file"));
        assert!(yaml.contains(&format!("path: '{}'", path.display())));
    }

    #[test]
    fn bundled_public_dashboards_are_valid_and_private_variant_is_excluded() {
        let pool: serde_json::Value = serde_json::from_str(POOL_DASHBOARD).unwrap();
        let users: serde_json::Value = serde_json::from_str(USERS_DASHBOARD).unwrap();

        assert_eq!(pool["title"], "Pool");
        assert_eq!(users["title"], "Users and Workers");
        assert!(!POOL_DASHBOARD.contains("users_private"));
        assert!(!USERS_DASHBOARD.contains("users_private"));
    }

    #[test]
    fn provisioning_writes_datasource_provider_and_dashboard_assets() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("grafana");

        let output = write_grafana_provisioning_in(&root, DEFAULT_PROMETHEUS_URL).unwrap();

        assert!(output.datasource.is_file());
        assert!(output.dashboard_provider.is_file());
        assert_eq!(output.dashboards.len(), 2);
        assert!(output.dashboards.iter().all(|path| path.is_file()));

        assert_eq!(
            std::fs::read_to_string(&output.dashboards[0]).unwrap(),
            POOL_DASHBOARD
        );
        assert_eq!(
            std::fs::read_to_string(&output.dashboards[1]).unwrap(),
            USERS_DASHBOARD
        );

        assert!(
            std::fs::read_to_string(&output.datasource)
                .unwrap()
                .contains(DEFAULT_PROMETHEUS_URL)
        );
        assert!(
            std::fs::read_to_string(&output.dashboard_provider)
                .unwrap()
                .contains(&root.join("dashboards").to_string_lossy().to_string())
        );
    }

    #[test]
    fn provisioning_reports_filesystem_errors() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("grafana");

        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("provisioning"), "blocks directory creation").unwrap();

        let error = write_grafana_provisioning_in(&root, DEFAULT_PROMETHEUS_URL).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("could not create Grafana directory")
        );
    }
}
