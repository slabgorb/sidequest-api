# sidequest-server

axum HTTP/WebSocket server — the entry point for the SideQuest game API.

## Routes

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/genres` | List available genre packs and their worlds |
| GET | `/ws` | WebSocket upgrade for game sessions |

## Design

The server depends on the `GameService` trait facade from `sidequest-agents`,
never on game internals directly. This keeps the transport layer thin:

```rust
AppState::new_with_game_service(
    Box::new(Orchestrator::new()),
    genre_packs_path,
)
```

Middleware:
- CORS for React dev server (`localhost:5173`)
- tower-http tracing

## Status

The server skeleton is functional — genre listing works, WebSocket accepts
connections. Full session state machine (Connect → Create → Play) is in
progress (Epic 2).

See [`docs/api-contract.md`](../../../docs/api-contract.md) for the full protocol spec.
