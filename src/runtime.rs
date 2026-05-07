use std::time::Instant;

use crate::config::EdgeConfig;
use crate::json::json_escape;
use crate::metrics::TrafficRegistry;
use crate::sidecar::{SidecarApplyReport, SidecarManager, SidecarPlan};

#[derive(Debug)]
pub struct EdgeState {
    started_at: Instant,
    config: EdgeConfig,
    traffic: TrafficRegistry,
    sidecars: SidecarManager,
}

impl EdgeState {
    pub fn new(config: EdgeConfig) -> Self {
        Self {
            started_at: Instant::now(),
            config,
            traffic: TrafficRegistry::default(),
            sidecars: SidecarManager::default(),
        }
    }

    pub fn config(&self) -> &EdgeConfig {
        &self.config
    }

    pub fn traffic(&self) -> &TrafficRegistry {
        &self.traffic
    }

    pub fn health_json(&self) -> String {
        format!(
            "{{\"status\":\"ok\",\"version\":\"{}\",\"uptime_seconds\":{}}}",
            env!("CARGO_PKG_VERSION"),
            self.started_at.elapsed().as_secs()
        )
    }

    pub fn metrics_json(&self) -> String {
        let totals = self.traffic.totals();
        let users = self
            .traffic
            .all()
            .into_iter()
            .map(|(user, traffic)| {
                format!(
                    "{{\"user\":\"{}\",\"upload_bytes\":{},\"download_bytes\":{}}}",
                    json_escape(&user),
                    traffic.upload_bytes,
                    traffic.download_bytes
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"upload_bytes\":{},\"download_bytes\":{},\"users\":[{}]}}",
            totals.upload_bytes, totals.download_bytes, users
        )
    }

    pub fn sidecars_json(&self) -> String {
        let plan = SidecarPlan::from_config(&self.config);
        self.sidecars.to_json(&plan)
    }

    pub fn reload_sidecars(&self) -> SidecarApplyReport {
        let plan = SidecarPlan::from_config(&self.config);
        self.sidecars.apply_plan(&plan)
    }
}

#[cfg(test)]
mod tests {
    use super::EdgeState;
    use crate::config::EdgeConfig;

    #[test]
    fn renders_metrics_json() {
        let state = EdgeState::new(EdgeConfig::starter());
        state.traffic().record("tag:user", 100, 200);

        let json = state.metrics_json();

        assert!(json.contains("\"upload_bytes\":100"));
        assert!(json.contains("\"download_bytes\":200"));
        assert!(json.contains("\"tag:user\""));
    }

    #[test]
    fn reload_sidecars_applies_disabled_starter_plan() {
        let state = EdgeState::new(EdgeConfig::starter());

        let report = state.reload_sidecars();
        let json = state.sidecars_json();

        assert!(report.started.is_empty());
        assert!(report.failed.is_empty());
        assert!(json.contains("\"state\":\"disabled\""));
    }
}
