// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::app::{App, AppAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// View state for the Monitoring screen.
#[derive(Debug, Clone)]
pub struct MonitoringView {
    /// The result of the last Prometheus and Grafana config generation.
    /// `None` means no attempt has been made yet.
    pub last_result: Option<Result<crate::monitoring::MonitoringOutputPaths, String>>,
    pub service_status: Option<(String, String)>,
    pub service_error: Option<String>,
}

impl MonitoringView {
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_result: None,
            service_status: None,
            service_error: None,
        }
    }

    /// Handles key input when the Monitoring screen has focus.
    pub fn handle_input(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char(character) => {
                let uppercase =
                    character.is_ascii_uppercase() || key.modifiers.contains(KeyModifiers::SHIFT);
                match character.to_ascii_lowercase() {
                    'g' if !uppercase => AppAction::GenerateMonitoringConfigs,
                    's' if uppercase => AppAction::StartMonitoringGrafana,
                    's' => AppAction::StartMonitoringPrometheus,
                    'x' if uppercase => AppAction::StopMonitoringGrafana,
                    'x' => AppAction::StopMonitoringPrometheus,
                    'r' if uppercase => AppAction::RestartMonitoringGrafana,
                    'r' => AppAction::RestartMonitoringPrometheus,
                    't' if !uppercase => AppAction::RefreshMonitoringStatus,
                    _ => AppAction::None,
                }
            }
            _ => AppAction::None,
        }
    }

    /// Renders the Monitoring screen.
    pub fn render(f: &mut Frame, app: &App, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        // ── Header ────────────────────────────────────────────────────
        lines.push(Line::from(Span::styled(
            "Monitoring — Prometheus and Grafana Configuration",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        if let Some((prometheus, grafana)) = &app.monitoring_view.service_status {
            lines.push(Line::from(format!("Prometheus service: {prometheus}")));
            lines.push(Line::from(format!("Grafana service   : {grafana}")));
        } else {
            lines.push(Line::from(
                "Service status not queried. Press [t] to check.",
            ));
        }
        if let Some(error) = &app.monitoring_view.service_error {
            lines.push(Line::from(Span::styled(
                format!("Service error: {error}"),
                Style::default().fg(Color::Red),
            )));
        }
        lines.push(Line::from(""));

        // ── Config availability ───────────────────────────────────────
        if app.p2pool_config.is_none() {
            lines.push(Line::from(Span::styled(
                "No P2Pool config loaded.",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(Span::styled(
                "Select a P2Pool config file first (P2Pool Config screen).",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            // Show the target that will be used.
            if let Some(cfg) = &app.p2pool_config {
                lines.push(Line::from(format!(
                    "Prometheus target : {}:{}",
                    cfg.api.hostname, cfg.api.port
                )));

                let auth_note = match (&cfg.api.auth_user, &cfg.api.auth_password) {
                    (Some(user), Some(_)) if !user.is_empty() => {
                        format!("Authentication    : basic_auth (user: {})", user)
                    }
                    _ => "Authentication    : none".to_string(),
                };
                lines.push(Line::from(auth_note));
            }

            lines.push(Line::from(""));

            // Show output paths even before generation.
            match crate::monitoring::prometheus_yml_path() {
                Ok(path) => lines.push(Line::from(format!(
                    "Prometheus config : {}",
                    path.display()
                ))),
                Err(e) => lines.push(Line::from(Span::styled(
                    format!("Prometheus path error: {e}"),
                    Style::default().fg(Color::Red),
                ))),
            }
            match crate::monitoring::monitoring_config_dir() {
                Ok(path) => lines.push(Line::from(format!(
                    "Grafana provisioning: {}",
                    path.join("grafana").join("provisioning").display()
                ))),
                Err(e) => lines.push(Line::from(Span::styled(
                    format!("Grafana path error: {e}"),
                    Style::default().fg(Color::Red),
                ))),
            }

            lines.push(Line::from(""));

            // ── Last generation result ─────────────────────────────────
            match &app.monitoring_view.last_result {
                None => {
                    lines.push(Line::from(Span::styled(
                        "Monitoring files not generated yet. Press [g] to generate.",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                Some(Ok(path)) => {
                    lines.push(Line::from(Span::styled(
                        "✓ Prometheus and Grafana files generated.",
                        Style::default().fg(Color::Green),
                    )));
                    lines.push(Line::from(format!(
                        "  Prometheus: {}",
                        path.prometheus.display()
                    )));
                    lines.push(Line::from(format!(
                        "  Datasource: {}",
                        path.grafana.datasource.display()
                    )));
                    lines.push(Line::from(format!(
                        "  Provider  : {}",
                        path.grafana.dashboard_provider.display()
                    )));
                    lines.push(Line::from(format!(
                        "  Dashboards: {}",
                        path.grafana.dashboards.len()
                    )));
                    for dashboard in &path.grafana.dashboards {
                        lines.push(Line::from(format!("    {}", dashboard.display())));
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "[g] Regenerate",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                Some(Err(e)) => {
                    lines.push(Line::from(Span::styled(
                        "✗ Failed to generate monitoring files:",
                        Style::default().fg(Color::Red),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("  {e}"),
                        Style::default().fg(Color::Red),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "[g] Retry",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Optional — Native systemd user services",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(
            "[s/x/r] Prometheus start/stop/restart   [S/X/R] Grafana",
        ));
        lines.push(Line::from(
            "[t] Refresh status. Services start on demand and are not enabled at login.",
        ));
        lines.push(Line::from(
            "Generate config with [g] first. Install Prometheus and Grafana separately.",
        ));
        lines.push(Line::from(
            "Grafana is loopback-only; anonymous access is off. Change the admin password at first sign-in.",
        ));

        let paragraph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Monitoring "))
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }
}

impl Default for MonitoringView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn render_view(app: &App) -> String {
        render_view_at(app, 100, 30)
    }

    fn render_view_at(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                MonitoringView::render(f, app, area);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn service_action_feedback_is_visible_in_a_small_terminal() {
        let mut app = App::new();
        app.p2pool_config = Some(
            p2poolv2_config::Config::load(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/p2pool.toml"
            ))
            .unwrap(),
        );
        let long_path = std::path::PathBuf::from(format!(
            "/tmp/{}/monitoring/grafana/dashboards",
            "long-path-segment".repeat(8)
        ));
        app.monitoring_view.last_result = Some(Ok(crate::monitoring::MonitoringOutputPaths {
            prometheus: long_path.join("prometheus.yml"),
            grafana: crate::monitoring::GrafanaProvisioningPaths {
                datasource: long_path.join("provisioning/datasources/prometheus.yml"),
                dashboard_provider: long_path.join("provisioning/dashboards/dashboards.yml"),
                dashboards: vec![
                    long_path.join("pool.json"),
                    long_path.join("users.json"),
                    long_path.join("extra.json"),
                ],
            },
        }));
        app.monitoring_view.service_status = Some(("inactive".to_string(), "inactive".to_string()));
        app.monitoring_view.service_error =
            Some("Prometheus start failed: service manager unavailable".to_string());

        let output = render_view_at(&app, 80, 24);

        assert!(output.contains("Prometheus start failed"));
        assert!(output.contains("inactive"));
    }

    #[test]
    fn new_starts_with_no_result() {
        let view = MonitoringView::new();
        assert!(view.last_result.is_none());
    }

    #[test]
    fn g_key_returns_generate_action() {
        let mut view = MonitoringView::new();
        let action = view.handle_input(key(KeyCode::Char('g')));
        assert!(matches!(action, AppAction::GenerateMonitoringConfigs));
    }

    #[test]
    fn monitoring_keys_return_service_actions() {
        let mut view = MonitoringView::new();
        assert!(matches!(
            view.handle_input(key(KeyCode::Char('s'))),
            AppAction::StartMonitoringPrometheus
        ));
        assert!(matches!(
            view.handle_input(key(KeyCode::Char('X'))),
            AppAction::StopMonitoringGrafana
        ));
        assert!(matches!(
            view.handle_input(key(KeyCode::Char('t'))),
            AppAction::RefreshMonitoringStatus
        ));
    }

    #[test]
    fn shifted_lowercase_s_starts_grafana() {
        let mut view = MonitoringView::new();
        let shifted_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SHIFT);
        assert!(matches!(
            view.handle_input(shifted_s),
            AppAction::StartMonitoringGrafana
        ));
    }

    #[test]
    fn other_key_returns_none_action() {
        let mut view = MonitoringView::new();
        let action = view.handle_input(key(KeyCode::Char('z')));
        assert!(matches!(action, AppAction::None));
    }

    #[test]
    fn render_shows_no_config_message_when_config_missing() {
        let app = App::new();
        let output = render_view(&app);
        assert!(output.contains("No P2Pool config loaded"));
    }

    #[test]
    fn render_shows_generate_hint_when_no_result_yet() {
        let mut app = App::new();
        // Provide a minimal config so we bypass the "no config" branch.
        // We set p2pool_config by directly assigning to the field via
        // a fixture config loaded from disk.
        let cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        app.p2pool_config = Some(cfg);
        let output = render_view(&app);
        assert!(output.contains("Monitoring files not generated yet"));
    }

    #[test]
    fn render_shows_success_after_successful_generation() {
        let mut app = App::new();
        let cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        app.p2pool_config = Some(cfg);
        app.monitoring_view.last_result = Some(Ok(crate::monitoring::MonitoringOutputPaths {
            prometheus: std::path::PathBuf::from("/tmp/prometheus.yml"),
            grafana: crate::monitoring::GrafanaProvisioningPaths {
                datasource: "/tmp/grafana/provisioning/datasources/prometheus.yml".into(),
                dashboard_provider: "/tmp/grafana/provisioning/dashboards/dashboards.yml".into(),
                dashboards: vec![
                    "/tmp/grafana/dashboards/pool.json".into(),
                    "/tmp/grafana/dashboards/users.json".into(),
                ],
            },
        }));
        let output = render_view(&app);
        assert!(output.contains("files generated"));
        assert!(output.contains("prometheus.yml"));
    }

    #[test]
    fn render_shows_error_after_failed_generation() {
        let mut app = App::new();
        let cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        app.p2pool_config = Some(cfg);
        app.monitoring_view.last_result = Some(Err("permission denied".to_string()));
        let output = render_view(&app);
        assert!(output.contains("permission denied"));
    }

    #[test]
    fn render_shows_prometheus_target_from_config() {
        let mut app = App::new();
        let mut cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        cfg.api.hostname = "192.0.2.5".to_string();
        cfg.api.port = 9090;
        app.p2pool_config = Some(cfg);
        let output = render_view(&app);
        assert!(output.contains("192.0.2.5:9090"));
    }

    #[test]
    fn render_shows_no_auth_note_when_credentials_absent() {
        let mut app = App::new();
        let cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        app.p2pool_config = Some(cfg);
        let output = render_view(&app);
        assert!(output.contains("Authentication    : none"));
    }

    #[test]
    fn render_shows_auth_note_when_credentials_present() {
        let mut app = App::new();
        let mut cfg = p2poolv2_config::Config::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/p2pool.toml"
        ))
        .unwrap();
        cfg.api.auth_user = Some("prometheus-user".to_string());
        cfg.api.auth_password = Some("s3cr3t".to_string());
        app.p2pool_config = Some(cfg);
        let output = render_view(&app);
        assert!(output.contains("basic_auth"));
        assert!(output.contains("prometheus-user"));
        // Password must not appear in the TUI.
        assert!(!output.contains("s3cr3t"));
    }

    #[test]
    fn render_shows_optional_monitoring_note() {
        let app = App::new();
        let output = render_view(&app);
        assert!(output.contains("Optional"));
        assert!(output.contains("Install Prometheus and Grafana separately"));
    }
}
