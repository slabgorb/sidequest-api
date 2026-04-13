# sidequest-server

axum HTTP/WebSocket server — the entry point for the SideQuest game API.
Integration layer that wires all other crates together.

## Routes

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/genres` | List available genre packs and their worlds |
| GET | `/ws` | WebSocket upgrade for game sessions |
| GET | `/ws/watcher` | Read-only telemetry stream for GM panel |

## Design

The server depends on the `GameService` trait facade from `sidequest-agents`,
never on game internals directly. This keeps the transport layer thin:

```rust
AppState::new_with_game_service(
    Box::new(Orchestrator::new()),
    genre_packs_path,
)
```

Dispatch logic is split into `dispatch/` modules (audio, combat, connect, prompt,
render, session_sync, slash, state_mutations, tropes).

Middleware:
- CORS for React dev server (`localhost:5173`)
- tower-http tracing
- Structured telemetry via `tracing-subscriber` + `tracing-chrome`

## Key Components

- **Session state machine** — AwaitingConnect → Creating → Playing
- **SharedGameSession** — world-level state shared across multiplayer players
- **ProcessingGuard** — RAII guard preventing concurrent actions per player
- **WatcherEvent** — structured telemetry for GM panel (agent spans, state transitions, coverage gaps)
- **Render integration** — async image broadcaster with path rewriting and tier mapping

The authoritative protocol definition lives in the `sidequest-protocol`
crate (`crates/sidequest-protocol/src/`) — `GameMessage` and its typed payloads
are the contract. A higher-level cross-repo protocol reference lives in the
orchestrator repo at `orc-quest/docs/api-contract.md` (not checked into this
repo).
