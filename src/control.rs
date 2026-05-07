use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

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
        let listener = TcpListener::bind(&self.state.config().control.listen_addr)?;
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
            ("POST", "/reload") => HttpResponse::accepted("{\"accepted\":true}".to_string()),
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

    fn handle_stream(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer)?;
        let request = String::from_utf8_lossy(&buffer[..read]);
        let response = self.handle_request(&request);
        stream.write_all(response.to_http().as_bytes())
    }
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
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ControlServer;
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
}
