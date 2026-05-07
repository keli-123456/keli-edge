use crate::json::json_escape;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyUser {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NaiveCaddyConfig {
    pub listen: String,
    pub server_name: String,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub users: Vec<ProxyUser>,
    pub probe_resistance: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MieruServerConfig {
    pub port_binding: MieruPortBinding,
    pub transport: String,
    pub users: Vec<ProxyUser>,
    pub logging_level: String,
    pub mtu: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MieruPortBinding {
    Port(u16),
    PortRange(String),
}

pub fn generated_file(path: impl Into<String>, contents: impl Into<String>) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        contents: contents.into(),
    }
}

pub fn naive_caddyfile_file(path: impl Into<String>, config: &NaiveCaddyConfig) -> GeneratedFile {
    generated_file(path, render_naive_caddyfile(config))
}

pub fn mieru_server_config_file(
    path: impl Into<String>,
    config: &MieruServerConfig,
) -> GeneratedFile {
    generated_file(path, render_mieru_server_config(config))
}

pub fn render_naive_caddyfile(config: &NaiveCaddyConfig) -> String {
    let listen = if config.listen.trim().is_empty() {
        ":443".to_string()
    } else {
        config.listen.trim().to_string()
    };
    let server_name = config.server_name.trim();
    let site = if server_name.is_empty() {
        listen
    } else {
        format!("{}, {}", listen, server_name)
    };

    let tls = match (&config.cert_file, &config.key_file) {
        (Some(cert), Some(key)) if !cert.trim().is_empty() && !key.trim().is_empty() => {
            format!("    tls {} {}\n", caddy_token(cert), caddy_token(key))
        }
        _ => String::new(),
    };

    let users = config
        .users
        .iter()
        .map(|user| {
            format!(
                "            basic_auth {} {}\n",
                caddy_token(&user.username),
                caddy_token(&user.password)
            )
        })
        .collect::<String>();
    let probe_resistance = config
        .probe_resistance
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("            probe_resistance {}\n", caddy_token(value)))
        .unwrap_or_default();

    format!(
        "{{\n    order forward_proxy first\n}}\n\n{} {{\n{}    route {{\n        forward_proxy {{\n{}            hide_ip\n            hide_via\n{}        }}\n        respond \"OK\" 200\n    }}\n}}\n",
        site, tls, users, probe_resistance
    )
}

pub fn render_mieru_server_config(config: &MieruServerConfig) -> String {
    let binding = match &config.port_binding {
        MieruPortBinding::Port(port) => format!("\"port\":{}", port),
        MieruPortBinding::PortRange(port_range) => {
            format!("\"portRange\":\"{}\"", json_escape(port_range))
        }
    };
    let users = config
        .users
        .iter()
        .map(|user| {
            format!(
                "{{\"name\":\"{}\",\"password\":\"{}\"}}",
                json_escape(&user.username),
                json_escape(&user.password)
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{{\"portBindings\":[{{{},\"protocol\":\"{}\"}}],\"users\":[{}],\"loggingLevel\":\"{}\",\"mtu\":{}}}",
        binding,
        json_escape(&config.transport),
        users,
        json_escape(&config.logging_level),
        config.mtu
    )
}

fn caddy_token(value: &str) -> String {
    if value
        .chars()
        .all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '-' | '/' | ':' | '$')
        })
    {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::{
        naive_caddyfile_file, render_mieru_server_config, render_naive_caddyfile,
        MieruPortBinding, MieruServerConfig, NaiveCaddyConfig, ProxyUser,
    };

    #[test]
    fn renders_naive_caddyfile() {
        let config = NaiveCaddyConfig {
            listen: ":443".to_string(),
            server_name: "edge.example.test".to_string(),
            cert_file: Some("/etc/ssl/cert.pem".to_string()),
            key_file: Some("/etc/ssl/key.pem".to_string()),
            users: vec![ProxyUser {
                username: "alice".to_string(),
                password: "secret".to_string(),
            }],
            probe_resistance: Some("hidden.example.test".to_string()),
        };

        let output = render_naive_caddyfile(&config);

        assert!(output.contains("forward_proxy"));
        assert!(output.contains("basic_auth alice secret"));
        assert!(output.contains("probe_resistance hidden.example.test"));
        assert!(output.contains("tls /etc/ssl/cert.pem /etc/ssl/key.pem"));
    }

    #[test]
    fn renders_mieru_server_config() {
        let config = MieruServerConfig {
            port_binding: MieruPortBinding::PortRange("2100-2200".to_string()),
            transport: "TCP".to_string(),
            users: vec![ProxyUser {
                username: "bob".to_string(),
                password: "pw".to_string(),
            }],
            logging_level: "INFO".to_string(),
            mtu: 1400,
        };

        let output = render_mieru_server_config(&config);

        assert!(output.contains("\"portRange\":\"2100-2200\""));
        assert!(output.contains("\"protocol\":\"TCP\""));
        assert!(output.contains("\"name\":\"bob\""));
    }

    #[test]
    fn wraps_generated_files() {
        let file = naive_caddyfile_file(
            "runtime/naive/Caddyfile",
            &NaiveCaddyConfig {
                listen: ":443".to_string(),
                server_name: "edge.example.test".to_string(),
                cert_file: None,
                key_file: None,
                users: Vec::new(),
                probe_resistance: None,
            },
        );

        assert_eq!(file.path, "runtime/naive/Caddyfile");
        assert!(file.contents.contains("forward_proxy"));
    }
}
