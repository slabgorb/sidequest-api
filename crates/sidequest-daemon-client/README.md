# sidequest-daemon-client

Async client for communicating with
[sidequest-daemon](https://github.com/slabgorb/sidequest-daemon) over a Unix
domain socket. JSON-RPC style protocol (newline-delimited JSON).

## What's in here

- **`DaemonClient`** — Connect, send requests, read responses with timeout handling
- **`render()`** — Image generation (Flux) with OTEL lifecycle tracing
- **`synthesize()`** — Text-to-speech (Kokoro) returning PCM audio bytes
- **`embed()`** — Sentence embeddings for semantic lore retrieval
- **`warm_up()`** / **`ping()`** / **`shutdown()`** — Daemon lifecycle management

## Usage

```rust
use sidequest_daemon_client::{DaemonClient, DaemonConfig, RenderParams};

let mut client = DaemonClient::connect(DaemonConfig::default()).await?;
client.ping().await?;

let result = client.render(RenderParams {
    prompt: "A weathered tavern at dusk".into(),
    art_style: "oil_painting".into(),
    tier: "scene_illustration".into(),
    ..Default::default()
}).await?;
```

The daemon itself is a Python process — see
[sidequest-daemon](https://github.com/slabgorb/sidequest-daemon) for setup.
