use std::collections::{HashMap, HashSet};
use std::process::{Child, Command};
use std::sync::{Mutex, RwLock};

use crate::config::{EdgeConfig, SidecarConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarPlan {
    specs: Vec<SidecarConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarStatus {
    pub name: String,
    pub protocol: String,
    pub enabled: bool,
    pub state: SidecarState,
    pub pid: Option<u32>,
    pub command: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidecarState {
    Disabled,
    Running,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidecarApplyReport {
    pub started: Vec<String>,
    pub stopped: Vec<String>,
    pub failed: Vec<SidecarFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarFailure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct SidecarManager {
    processes: Mutex<HashMap<String, RunningSidecar>>,
    statuses: RwLock<HashMap<String, SidecarStatus>>,
}

#[derive(Debug)]
struct RunningSidecar {
    command: String,
    child: Child,
}

enum ExistingSidecarAction {
    Missing,
    KeepRunning(u32),
    Restart,
    Failed(String),
}

impl SidecarPlan {
    pub fn from_config(config: &EdgeConfig) -> Self {
        Self {
            specs: config.sidecars.clone(),
        }
    }

    pub fn specs(&self) -> &[SidecarConfig] {
        &self.specs
    }

    pub fn command_preview(spec: &SidecarConfig) -> String {
        let mut parts = vec![spec.binary.clone()];
        parts.extend(spec.args.clone());
        parts.join(" ")
    }

    pub fn to_json(&self) -> String {
        let statuses = self
            .specs
            .iter()
            .map(SidecarStatus::from_spec)
            .collect::<Vec<_>>();
        statuses_json(&statuses)
    }
}

impl SidecarManager {
    pub fn apply_plan(&self, plan: &SidecarPlan) -> SidecarApplyReport {
        let mut report = SidecarApplyReport::default();
        let mut processes = self.processes.lock().expect("sidecar process lock poisoned");
        let mut statuses = self.statuses.write().expect("sidecar status lock poisoned");
        let desired_enabled = plan
            .specs()
            .iter()
            .filter(|spec| spec.enabled)
            .map(|spec| spec.name.clone())
            .collect::<HashSet<_>>();

        let running_names = processes.keys().cloned().collect::<Vec<_>>();
        for name in running_names {
            if !desired_enabled.contains(&name) {
                if let Some(mut running) = processes.remove(&name) {
                    stop_child(&mut running.child);
                    report.stopped.push(name);
                }
            }
        }

        statuses.clear();
        for spec in plan.specs() {
            if !spec.enabled {
                statuses.insert(spec.name.clone(), SidecarStatus::from_spec(spec));
                continue;
            }

            let existing_action = if let Some(running) = processes.get_mut(&spec.name) {
                let command = SidecarPlan::command_preview(spec);
                match running.child.try_wait() {
                    Ok(None) if running.command == command => ExistingSidecarAction::KeepRunning(running.child.id()),
                    Ok(None) | Ok(Some(_)) => ExistingSidecarAction::Restart,
                    Err(err) => {
                        ExistingSidecarAction::Failed(format!("inspect process failed: {err}"))
                    }
                }
            } else {
                ExistingSidecarAction::Missing
            };

            match existing_action {
                ExistingSidecarAction::KeepRunning(pid) => {
                    statuses.insert(spec.name.clone(), SidecarStatus::running(spec, pid));
                    continue;
                }
                ExistingSidecarAction::Restart => {
                    if let Some(mut running) = processes.remove(&spec.name) {
                        stop_child(&mut running.child);
                        report.stopped.push(spec.name.clone());
                    }
                }
                ExistingSidecarAction::Failed(error) => {
                    processes.remove(&spec.name);
                    report.failed.push(SidecarFailure {
                        name: spec.name.clone(),
                        error: error.clone(),
                    });
                    statuses.insert(spec.name.clone(), SidecarStatus::failed(spec, error));
                    continue;
                }
                ExistingSidecarAction::Missing => {}
            }

            match spawn_sidecar(spec) {
                Ok(running) => {
                    let pid = running.child.id();
                    processes.insert(spec.name.clone(), running);
                    report.started.push(spec.name.clone());
                    statuses.insert(spec.name.clone(), SidecarStatus::running(spec, pid));
                }
                Err(error) => {
                    report.failed.push(SidecarFailure {
                        name: spec.name.clone(),
                        error: error.clone(),
                    });
                    statuses.insert(spec.name.clone(), SidecarStatus::failed(spec, error));
                }
            }
        }

        report
    }

    pub fn to_json(&self, plan: &SidecarPlan) -> String {
        let statuses = self.statuses.read().expect("sidecar status lock poisoned");
        if statuses.is_empty() {
            return plan.to_json();
        }

        let mut values = statuses.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| left.name.cmp(&right.name));
        statuses_json(&values)
    }
}

impl Drop for SidecarManager {
    fn drop(&mut self) {
        if let Ok(mut processes) = self.processes.lock() {
            for (_, mut running) in processes.drain() {
                stop_child(&mut running.child);
            }
        }
    }
}

impl SidecarStatus {
    pub fn from_spec(spec: &SidecarConfig) -> Self {
        Self {
            name: spec.name.clone(),
            protocol: spec.protocol.as_str().to_string(),
            enabled: spec.enabled,
            state: if spec.enabled {
                SidecarState::Stopped
            } else {
                SidecarState::Disabled
            },
            pid: None,
            command: SidecarPlan::command_preview(spec),
            error: None,
        }
    }

    pub fn running(spec: &SidecarConfig, pid: u32) -> Self {
        Self {
            name: spec.name.clone(),
            protocol: spec.protocol.as_str().to_string(),
            enabled: true,
            state: SidecarState::Running,
            pid: Some(pid),
            command: SidecarPlan::command_preview(spec),
            error: None,
        }
    }

    pub fn failed(spec: &SidecarConfig, error: String) -> Self {
        Self {
            name: spec.name.clone(),
            protocol: spec.protocol.as_str().to_string(),
            enabled: true,
            state: SidecarState::Failed,
            pid: None,
            command: SidecarPlan::command_preview(spec),
            error: Some(error),
        }
    }
}

impl SidecarState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

impl SidecarApplyReport {
    pub fn to_json(&self) -> String {
        let started = json_string_array(&self.started);
        let stopped = json_string_array(&self.stopped);
        let failed = self
            .failed
            .iter()
            .map(|failure| {
                format!(
                    "{{\"name\":\"{}\",\"error\":\"{}\"}}",
                    json_escape(&failure.name),
                    json_escape(&failure.error)
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"started\":{},\"stopped\":{},\"failed\":[{}]}}",
            started, stopped, failed
        )
    }
}

fn spawn_sidecar(spec: &SidecarConfig) -> Result<RunningSidecar, String> {
    let binary = spec.binary.trim();
    if binary.is_empty() {
        return Err("sidecar binary is empty".to_string());
    }

    let mut command = Command::new(binary);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }

    let child = command
        .spawn()
        .map_err(|err| format!("spawn {} failed: {err}", spec.name))?;

    Ok(RunningSidecar {
        command: SidecarPlan::command_preview(spec),
        child,
    })
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn statuses_json(statuses: &[SidecarStatus]) -> String {
    let sidecars = statuses
        .iter()
        .map(|status| {
            let pid = status
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string());
            let error = status
                .error
                .as_ref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_string());

            format!(
                "{{\"name\":\"{}\",\"protocol\":\"{}\",\"enabled\":{},\"state\":\"{}\",\"pid\":{},\"command\":\"{}\",\"error\":{}}}",
                json_escape(&status.name),
                json_escape(&status.protocol),
                status.enabled,
                status.state.as_str(),
                pid,
                json_escape(&status.command),
                error
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!("{{\"sidecars\":[{}]}}", sidecars)
}

fn json_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");

    format!("[{}]", values)
}

pub fn json_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{SidecarManager, SidecarPlan};
    use crate::config::{EdgeConfig, SidecarConfig};
    use crate::protocol::Protocol;

    #[test]
    fn renders_sidecar_plan_json() {
        let config = EdgeConfig::starter();
        let plan = SidecarPlan::from_config(&config);
        let json = plan.to_json();

        assert!(json.contains("\"naive-caddy\""));
        assert!(json.contains("\"mieru-mita\""));
        assert!(json.contains("\"enabled\":false"));
        assert!(json.contains("\"state\":\"disabled\""));
    }

    #[test]
    fn manager_marks_disabled_sidecars_without_spawning() {
        let config = EdgeConfig::starter();
        let plan = SidecarPlan::from_config(&config);
        let manager = SidecarManager::default();

        let report = manager.apply_plan(&plan);
        let json = manager.to_json(&plan);

        assert!(report.started.is_empty());
        assert!(report.failed.is_empty());
        assert!(json.contains("\"state\":\"disabled\""));
    }

    #[test]
    fn manager_reports_spawn_failures() {
        let config = EdgeConfig {
            sidecars: vec![SidecarConfig {
                name: "missing-naive".to_string(),
                protocol: Protocol::Naive,
                enabled: true,
                binary: "keli-edge-missing-binary-for-test".to_string(),
                args: Vec::new(),
                env: Vec::new(),
            }],
            ..EdgeConfig::starter()
        };
        let plan = SidecarPlan::from_config(&config);
        let manager = SidecarManager::default();

        let report = manager.apply_plan(&plan);
        let json = manager.to_json(&plan);

        assert!(report.started.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert!(json.contains("\"state\":\"failed\""));
        assert!(json.contains("missing-naive"));
    }
}
