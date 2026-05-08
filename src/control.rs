use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use crate::config::SidecarConfig;
use crate::protocol::Protocol;
use crate::render::generated_file;
use crate::runtime::EdgeState;

#[derive(Clone)]
pub struct ControlServer {
    state: Arc<EdgeState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
    pub body: String,
}

impl ControlServer {
    pub fn new(state: Arc<EdgeState>) -> Self {
        Self { state }
    }

    pub fn serve(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(self.state.control_listen_addr())?;
        for stream in listener.incoming() {
            let stream = stream?;
            let server = self.clone();
            thread::spawn(move || {
                let _ = server.handle_stream(stream);
            });
        }
        Ok(())
    }

    pub fn handle_request(&self, raw_request: &str) -> HttpResponse {
        let mut lines = raw_request.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts.next().unwrap_or_default();
        let target = request_parts.next().unwrap_or_default();
        let path = target.split('?').next().unwrap_or(target);
        let body = raw_request.split("\r\n\r\n").nth(1).unwrap_or_default();

        match (method, path) {
            ("GET", "/health") => HttpResponse::ok(self.state.health_json()),
            ("GET", "/metrics") => HttpResponse::ok(self.state.metrics_json()),
            ("GET", "/sidecars") => HttpResponse::ok(self.state.sidecars_json()),
            ("POST", "/reload") => HttpResponse::accepted(self.state.reload_sidecars().to_json()),
            ("POST", "/sidecars/upsert") => self.upsert_sidecar(body),
            ("POST", "/traffic/drain") => HttpResponse::ok(self.state.drain_metrics_json()),
            ("POST", "/traffic") => self.record_traffic(body),
            _ => HttpResponse::not_found(),
        }
    }

    fn record_traffic(&self, body: &str) -> HttpResponse {
        let form = parse_form(body);
        let user = form.get("user").cloned().unwrap_or_default();
        let upload = form.get("upload").and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
        let download = form
            .get("download")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);

        if user.is_empty() {
            return HttpResponse::bad_request("{\"error\":\"user is required\"}".to_string());
        }

        self.state.traffic().record(user, upload, download);
        HttpResponse::ok("{\"recorded\":true}".to_string())
    }

    fn upsert_sidecar(&self, body: &str) -> HttpResponse {
        let form = parse_form(body);
        let name = form.get("name").cloned().unwrap_or_default();
        let protocol = match form.get("protocol").map(String::as_str) {
            Some(value) => match Protocol::from_str(value) {
                Ok(protocol) => protocol,
                Err(error) => {
                    return HttpResponse::bad_request(format!(
                        "{{\"error\":\"{}\"}}",
                        crate::json::json_escape(&error)
                    ))
                }
            },
            None => {
                return HttpResponse::bad_request(
                    "{\"error\":\"protocol is required\"}".to_string(),
                );
            }
        };
        let binary = form.get("binary").cloned().unwrap_or_default();
        if name.trim().is_empty() {
            return HttpResponse::bad_request("{\"error\":\"name is required\"}".to_string());
        }
        if binary.trim().is_empty() {
            return HttpResponse::bad_request("{\"error\":\"binary is required\"}".to_string());
        }

        let sidecar = SidecarConfig {
            name: name.trim().to_string(),
            protocol,
            enabled: form_bool(&form, "enabled"),
            binary: binary.trim().to_string(),
            args: form_lines(&form, "args"),
            env: form_key_value_lines(&form, "env"),
            generated_files: form_generated_files(&form),
        };
        HttpResponse::accepted(self.state.upsert_sidecar(sidecar).to_json())
    }

    fn handle_stream(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer)?;
        let request = String::from_utf8_lossy(&buffer[..read]);
        let response = self.handle_request(&request);
        stream.write_all(response.to_http().as_bytes())
    }
}

fn form_bool(form: &HashMap<String, String>, key: &str) -> bool {
    let Some(value) = form.get(key) else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn form_lines(form: &HashMap<String, String>, key: &str) -> Vec<String> {
    form.get(key)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn form_key_value_lines(form: &HashMap<String, String>, key: &str) -> Vec<(String, String)> {
    form.get(key)
        .map(|value| {
            value
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once('=')?;
                    let key = key.trim();
                    if key.is_empty() {
                        return None;
                    }
                    Some((key.to_string(), value.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn form_generated_files(form: &HashMap<String, String>) -> Vec<crate::render::GeneratedFile> {
    let mut files = Vec::new();
    for index in 0.. {
        let path_key = if index == 0 {
            "file_path".to_string()
        } else {
            format!("file_path_{index}")
        };
        let contents_key = if index == 0 {
            "file_contents".to_string()
        } else {
            format!("file_contents_{index}")
        };
        let Some(path) = form.get(&path_key) else {
            break;
        };
        if path.trim().is_empty() {
            continue;
        }
        let contents = form.get(&contents_key).cloned().unwrap_or_default();
        files.push(generated_file(path.trim(), contents));
    }
    files
}

impl HttpResponse {
    pub fn ok(body: String) -> Self {
        Self {
            status: 200,
            reason: "OK",
            body,
        }
    }

    pub fn accepted(body: String) -> Self {
        Self {
            status: 202,
            reason: "Accepted",
            body,
        }
    }

    pub fn bad_request(body: String) -> Self {
        Self {
            status: 400,
            reason: "Bad Request",
            body,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            reason: "Not Found",
            body: "{\"error\":\"not found\"}".to_string(),
        }
    }

    pub fn to_http(&self) -> String {
        format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            self.status,
            self.reason,
            self.body.len(),
            self.body
        )
    }
}

fn parse_form(body: &str) -> HashMap<String, String> {
    body.split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((decode_form_component(key.trim()), decode_form_component(value.trim())))
        })
        .collect()
}

fn decode_form_component(value: &str) -> String {
    let mut output = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let Some(decoded) = decode_hex_byte(bytes[index + 1], bytes[index + 2]) {
                    output.push(decoded);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            value => {
                output.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn decode_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_form, ControlServer};
    use crate::config::EdgeConfig;
    use crate::runtime::EdgeState;
    use std::sync::Arc;

    #[test]
    fn health_endpoint_returns_ok() {
        let server = ControlServer::new(Arc::new(EdgeState::new(EdgeConfig::starter())));

        let response = server.handle_request("GET /health HTTP/1.1\r\n\r\n");

        assert_eq!(response.status, 200);
        assert!(response.body.contains("\"status\":\"ok\""));
    }

    #[test]
    fn traffic_endpoint_records_user_bytes() {
        let state = Arc::new(EdgeState::new(EdgeConfig::starter()));
        let server = ControlServer::new(state.clone());

        let response = server.handle_request(
            "POST /traffic HTTP/1.1\r\ncontent-length: 33\r\n\r\nuser=u1&upload=9&download=11",
        );

        assert_eq!(response.status, 200);
        assert!(state.metrics_json().contains("\"upload_bytes\":9"));
        assert!(state.metrics_json().contains("\"download_bytes\":11"));
    }

    #[test]
    fn traffic_drain_endpoint_returns_and_clears_user_bytes() {
        let state = Arc::new(EdgeState::new(EdgeConfig::starter()));
        state.traffic().record("node:user", 10, 20);
        let server = ControlServer::new(state.clone());

        let response =
            server.handle_request("POST /traffic/drain HTTP/1.1\r\ncontent-length: 0\r\n\r\n");

        assert_eq!(response.status, 200);
        assert!(response.body.contains("\"node:user\""));
        assert!(response.body.contains("\"upload_bytes\":10"));
        assert!(state.metrics_json().contains("\"upload_bytes\":0"));
    }

    #[test]
    fn reload_endpoint_applies_sidecar_plan() {
        let server = ControlServer::new(Arc::new(EdgeState::new(EdgeConfig::starter())));

        let response = server.handle_request("POST /reload HTTP/1.1\r\ncontent-length: 0\r\n\r\n");

        assert_eq!(response.status, 202);
        assert!(response.body.contains("\"started\":[]"));
        assert!(response.body.contains("\"failed\":[]"));
    }

    #[test]
    fn sidecar_upsert_endpoint_updates_plan_and_applies_it() {
        let state = Arc::new(EdgeState::new(EdgeConfig::starter()));
        let server = ControlServer::new(state.clone());

        let response = server.handle_request(concat!(
            "POST /sidecars/upsert HTTP/1.1\r\ncontent-length: 120\r\n\r\n",
            "name=mieru-mita&protocol=mieru&enabled=false&binary=mita",
            "&args=run&env=MITA_CONFIG_JSON_FILE%3Druntime%2Fmieru.json"
        ));

        assert_eq!(response.status, 202);
        let json = state.sidecars_json();
        assert!(json.contains("\"mieru-mita\""));
        assert!(json.contains("\"command\":\"mita run\""));
    }

    #[test]
    fn form_parser_decodes_url_encoded_values() {
        let form = parse_form("user=node%3Auser+one&upload=1");

        assert_eq!(form.get("user").map(String::as_str), Some("node:user one"));
        assert_eq!(form.get("upload").map(String::as_str), Some("1"));
    }
}
