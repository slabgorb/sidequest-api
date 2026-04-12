# sidequest-daemon-client — Feature Inventory

Async Unix socket client for the Python media daemon. **~570 LOC, fully complete.**

## COMPLETE — Do Not Rewrite

- **DaemonClient** — `client.rs` — async client over `UnixStream` with JSON-RPC
  protocol. Methods: `ping()`, `render()`, `warm_up()`, `embed()`, `shutdown()`.
  Each method has OTEL tracing spans.
- **DaemonConfig** — socket path (`/tmp/sidequest-renderer.sock`), render timeout
  (300s), default timeout (10s).
- **RenderParams / RenderResult** — image generation request/response. `image_url`
  has NO `serde(default)` — missing path fails loudly. Accepts 5 field name aliases
  (`image_url`, `image_path`, `output_path`, `path`, `file`).
- **EmbedParams / EmbedResult** — sentence embeddings for semantic lore retrieval.
- **WarmUpParams / StatusResult** — pre-load daemon workers.
- **ErrorPayload** — flexible deserializer accepts both integer and string error codes.

## Important Constraints

- No silent fallbacks — empty `image_url` is rejected with a loud OTEL error.
- Render deserialization failures log raw JSON to the watcher for debugging.
- Zero TODO/FIXME — this crate is done.

## Historical

The TTS synthesis surface (`synthesize()`, `TtsParams`, `TtsResult`) was removed
when the Kokoro TTS pipeline was retired (Epic 27 / ADR-076). The daemon no longer
accepts `tts` or `music` tier routes for runtime inference.
