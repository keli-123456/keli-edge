# Keli Edge

`keli-edge` is the experimental Rust data-plane for Keli nodes.

The first goal is not to replace `kelinode`. `kelinode` remains the Go control-plane that talks to `keliboard`, pulls node config, reports traffic, and manages upgrades. `keli-edge` is a small Rust process that can grow into a high-performance local data-plane and sidecar supervisor.

## Scope

Current scaffold:

- Local control API over `127.0.0.1:17990`.
- Health, sidecar plan, traffic metrics, and reload endpoints.
- In-memory per-user traffic registry.
- Sidecar planning for protocols that should not be faked inside Xray, such as Naive and Mieru.
- No external Rust dependencies yet, so the first build stays offline-friendly.

Out of scope for this first cut:

- Reimplementing VMess, VLESS, Trojan, Hysteria2, TUIC, AnyTLS, Naive, or Mieru.
- Replacing `kelinode`.
- Running GPL protocol libraries inside this binary.

## Intended Architecture

```text
keliboard
   |
   | HTTPS / realtime
   v
kelinode (Go control-plane)
   |
   | localhost control API
   v
keli-edge (Rust data-plane / sidecar supervisor)
   |
   +-- caddy forwardproxy for naive
   +-- mita or other mieru-compatible sidecar
   +-- future Rust-native fast paths
```

## API

```text
GET  /health
GET  /metrics
GET  /sidecars
POST /traffic
POST /reload
```

`POST /traffic` currently accepts a simple form body:

```text
user=user-tag&upload=123&download=456
```

## Build

```bash
cargo test
cargo build --release
```

This Windows workspace currently does not have `cargo` installed, so validation should run on a Rust-enabled Linux/CI machine.

## Kelinode Config

`kelinode` can connect to this process through a local edge block:

```yaml
edge:
  enabled: true
  url: "http://127.0.0.1:17990"
  timeout: 2
```
