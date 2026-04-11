# sidequest-protocol

GameMessage enum and typed payloads for the SideQuest WebSocket protocol.

This is the leaf crate — no SideQuest dependencies. Everything else builds on top of it.

## What's in here

- **`GameMessage`** — Tagged enum covering 20+ message types between client and server
  (narration, combat, inventory, state deltas, audio cues, image delivery, etc.)
- **`NarrativePayload`** — Narration text + `StateDelta` + footnotes (`deny_unknown_fields`)
- **Validated newtypes** — `NonBlankString`, typed wrappers for IDs and constrained values
- **Input sanitization** — `sanitize_player_text()` strips XML, injection vectors, unicode confusables, zero-width chars
- **`PROTOCOL_VERSION`** — Version string for compatibility checking

## Usage

```rust
use sidequest_protocol::{GameMessage, sanitize_player_text, PROTOCOL_VERSION};

let clean = sanitize_player_text(raw_input);
```

The full protocol spec lives in [`docs/api-contract.md`](../../../docs/api-contract.md).
