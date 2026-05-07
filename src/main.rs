#![forbid(unsafe_code)]

use std::sync::Arc;

use keli_edge::config::EdgeConfig;
use keli_edge::control::ControlServer;
use keli_edge::runtime::EdgeState;

fn main() -> std::io::Result<()> {
    let config = EdgeConfig::starter();
    let listen_addr = config.control.listen_addr.clone();
    let state = Arc::new(EdgeState::new(config));
    let server = ControlServer::new(state);

    println!("keli-edge listening on {listen_addr}");
    server.serve()
}
