use crate::protocol::Protocol;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeConfig {
    pub control: ControlConfig,
    pub runtime_dir: String,
    pub sidecars: Vec<SidecarConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlConfig {
    pub listen_addr: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarConfig {
    pub name: String,
    pub protocol: Protocol,
    pub enabled: bool,
    pub binary: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl EdgeConfig {
    pub fn starter() -> Self {
        Self {
            control: ControlConfig::default(),
            runtime_dir: "runtime".to_string(),
            sidecars: vec![
                SidecarConfig {
                    name: "naive-caddy".to_string(),
                    protocol: Protocol::Naive,
                    enabled: false,
                    binary: "caddy".to_string(),
                    args: vec!["run".to_string(), "--config".to_string(), "runtime/naive/Caddyfile".to_string()],
                    env: Vec::new(),
                },
                SidecarConfig {
                    name: "mieru-mita".to_string(),
                    protocol: Protocol::Mieru,
                    enabled: false,
                    binary: "mita".to_string(),
                    args: vec!["run".to_string(), "--config".to_string(), "runtime/mieru/server.conf".to_string()],
                    env: Vec::new(),
                },
            ],
        }
    }
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:17990".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EdgeConfig;
    use crate::protocol::Protocol;

    #[test]
    fn starter_config_contains_sidecar_protocols_but_keeps_them_disabled() {
        let config = EdgeConfig::starter();

        assert_eq!(config.control.listen_addr, "127.0.0.1:17990");
        assert!(config.sidecars.iter().any(|sidecar| sidecar.protocol == Protocol::Naive));
        assert!(config.sidecars.iter().any(|sidecar| sidecar.protocol == Protocol::Mieru));
        assert!(config.sidecars.iter().all(|sidecar| !sidecar.enabled));
    }
}
