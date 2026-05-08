use std::time::Instant;

use crate::config::EdgeConfig;
use crate::json::json_escape;
use crate::metrics::{TrafficRegistry, TrafficSnapshot};
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
        traffic_json(self.traffic.all())
    }

    pub fn drain_metrics_json(&self) -> String {
        traffic_json(self.traffic.drain_all())
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

fn traffic_json(values: Vec<(String, TrafficSnapshot)>) -> String {
    let mut totals = TrafficSnapshot::default();
    let users = values
        .into_iter()
        .map(|(user, traffic)| {
            totals.upload_bytes = totals.upload_bytes.saturating_add(traffic.upload_bytes);
            totals.download_bytes = totals
                .download_bytes
                .saturating_add(traffic.download_bytes);
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

    #[test]
    fn drains_metrics_json_once() {
        let state = EdgeState::new(EdgeConfig::starter());
        state.traffic().record("tag:user", 100, 200);

        let drained = state.drain_metrics_json();
        let after = state.metrics_json();

        assert!(drained.contains("\"upload_bytes\":100"));
        assert!(drained.contains("\"tag:user\""));
        assert!(after.contains("\"upload_bytes\":0"));
        assert!(!after.contains("\"tag:user\""));
    }
}
