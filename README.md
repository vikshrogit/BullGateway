
# BullG — The 10× Faster API & AI Gateway (Rust) 1.0.1

BullG is a **high‑performance, multi‑protocol API & AI Gateway** inspired by Kong/Tyk, built in Rust for **throughput, tail‑latency, and safety**.  
It supports HTTP/HTTPS/WS/WSS today and is designed to extend to gRPC/TCP in future. BullG provides a **flexible plugin system** with **pre / post / intermediate** phases and supports plugin authoring in **Rust, Python, and JavaScript** out of the box. (C via TinyCC or prebuilt shared libs is scaffolded.)

> ⚠️ This repository is a **production‑grade skeleton** with working components and clear extension points. You can run it, route traffic, sync config from a control‑plane or From Tenants (WebSocket with HTTPS fallback), and execute built‑in plugins. For more info check BullG Anger Documents

## Release
v1.0.0 -> Base Version Released 13th August, 2025
v1.0.1 -> Tenant Plane Version Releasing in 28th September 2025

## Features

- Multi‑process + multithread runtime (Tokio) using all CPU cores
- TLS/mTLS (gateway & upstream) using `rustls`
- Config loader: `config.yaml` / `config.json` / `config.toml`
- Control‑plane sync over **WebSocket (mTLS)** with **HTTPS polling** fallback
- Fast stores: LMDB via `heed` (with in‑memory fallback)
- Services, Routes, Consumers, Apps, Applied Plugins at Global/Service/Route
- **Plugin system** with Pre / Post / Intermediate phases
- Built‑in plugins (Rust): CORS, BasicAuth, JWT, Request/Response Transformer, HTTP Log, Request Termination, GTLS, (stubs for OIDC, MTLS upstream, etc.)
- **bullg variable** (context) per request with helpers: headers/body/editors, tools.httpx, encryption utils, user vars
- Tracing & metrics via OpenTelemetry
- Structured logging with external HTTP log drain
- Hot‑reload sync every 5s on HTTPS fallback

## Quick Start

```bash
# 1) Build workspace
cargo build --release

# 2) Run gateway (auto‑detects config format)
cargo run -p src -- --config ./examples/config.yaml
```

See `examples/` for a working config and routes. Built‑in plugins are enabled in config and control‑plane state.

## Repository Layout

- `src/bullg` — binary launcher
- `src/bullg-gateway` — HTTP/WS server and request pipeline
- `src/bullg-core` — core domain models (Service, Route, Plugin, Consumer, etc.)
- `src/bullg-config` — config parsing & defaults
- `src/bullg-control-sync` — WebSocket + HTTPS sync client, token cache
- `src/bullg-plugin-api` — `bullg` context, Plugin trait, phases
- `src/bullg-plugins` — built‑in plugins (Rust)
- `src/bullg-memory` — LMDB store + memory fallback
- `src/bullg-utils` — crypto, custom encryption, helpers
- `src/bullg-logger` — async log drain & HTTP log queue
- `src/bullg-tracing` — telemetry setup

## License

MIT — see `LICENSE`.
