# Architecture

`keli-edge` is designed as a narrow local component. It should accept explicit local control from `kelinode`, supervise sidecar processes, and expose low-overhead traffic metrics back to `kelinode`.

## Why A Separate Process

Some protocols are not good fits for direct Xray-style inbound support.

- Naive is best represented by Caddy forwardproxy behavior rather than a plain HTTP proxy.
- Mieru has its own runtime and licensing considerations, so sidecar integration is safer than direct linking.
- Rust is useful for high-throughput local data-plane work without forcing the Go control-plane to own every hot path.

## First Milestones

1. Stable local API contract with `kelinode`.
2. Sidecar lifecycle model: plan, start, reload, stop, status.
3. Traffic accounting interface.
4. Naive sidecar integration through Caddy forwardproxy.
5. Mieru sidecar integration through `mita` or a compatible listener.
6. Optional Rust-native fast paths for simple protocols after metrics and lifecycle are stable.

## Compatibility Rules

- Do not expose a protocol as supported until it can authenticate real users.
- Do not break Docker node mode or binary node mode.
- Do not require GPL code to be linked into this binary.
- Prefer localhost-only control surfaces.
- Keep protocol-specific sidecar config generated from panel data, not hand-maintained files.

## Sidecar Lifecycle

The sidecar manager treats the panel-derived plan as the source of truth.

- `disabled`: configured in the plan but intentionally not started.
- `stopped`: enabled in the plan but not currently running.
- `running`: the external process was started and has a PID.
- `failed`: the external process could not be started or inspected.

This keeps Naive and Mieru honest: a protocol is not reported as active unless a real sidecar process is running. Missing binaries, bad paths, and invalid generated configs should surface through `/sidecars` instead of becoming silent node failures.
