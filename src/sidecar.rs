use crate::config::{EdgeConfig, SidecarConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarPlan {
    specs: Vec<SidecarConfig>,
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

    pub fn enabled(&self) -> Vec<&SidecarConfig> {
        self.specs.iter().filter(|spec| spec.enabled).collect()
    }

    pub fn command_preview(spec: &SidecarConfig) -> String {
        let mut parts = vec![spec.binary.clone()];
        parts.extend(spec.args.clone());
        parts.join(" ")
    }

    pub fn to_json(&self) -> String {
        let sidecars = self
            .specs
            .iter()
            .map(|spec| {
                format!(
                    "{{\"name\":\"{}\",\"protocol\":\"{}\",\"enabled\":{},\"command\":\"{}\"}}",
                    json_escape(&spec.name),
                    json_escape(spec.protocol.as_str()),
                    spec.enabled,
                    json_escape(&Self::command_preview(spec))
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!("{{\"sidecars\":[{}]}}", sidecars)
    }
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
    use super::SidecarPlan;
    use crate::config::EdgeConfig;

    #[test]
    fn renders_sidecar_plan_json() {
        let config = EdgeConfig::starter();
        let plan = SidecarPlan::from_config(&config);
        let json = plan.to_json();

        assert!(json.contains("\"naive-caddy\""));
        assert!(json.contains("\"mieru-mita\""));
        assert!(json.contains("\"enabled\":false"));
    }
}
