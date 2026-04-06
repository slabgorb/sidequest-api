# sidequest-telemetry — Feature Inventory

Game telemetry types and global broadcast channel. **~200 LOC, single file.**

Decoupled from `sidequest-server` so any crate can emit telemetry events without
depending on the server or `AppState`. The server initializes the global channel
at startup and wires it to the `/ws/watcher` WebSocket endpoint.

## COMPLETE — Do Not Rewrite

- **WatcherEvent** — timestamp, component, event_type, severity, fields bag
- **WatcherEventType** — AgentSpanOpen/Close, StateTransition, TurnComplete, etc.
- **Severity** — Info, Warn, Error
- **WatcherEventBuilder** — fluent builder with `.field()`, `.field_opt()`, `.severity()`, `.send()`
- **Global channel** — `init_global_channel()`, `subscribe_global()`, `emit()`
- **`watcher!` macro** — one-line telemetry emission

## Usage

```rust
use sidequest_telemetry::watcher;

// Simple event
watcher!("combat", StateTransition, action = "combat_ended");

// Multiple fields
watcher!("combat", StateTransition,
    action = "hp_change",
    target = target_name,
    delta = -4
);

// With severity
watcher!("combat", StateTransition, Warn, action = "player_dead");

// Builder (for complex cases)
WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
    .field("action", "combat_tick")
    .field_opt("override", &maybe_value)
    .severity(Severity::Warn)
    .send();
```

## Architecture

- `OnceLock<broadcast::Sender>` — initialized once at server startup
- `.send()` is a no-op if channel not initialized (CLI tools, unit tests)
- Server spawns a history-capture task that stores events for late-joining GM panels
- Zero overhead when no subscribers are connected
