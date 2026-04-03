# sidequest-server — Feature Inventory

axum HTTP/WebSocket server. **~8,700 LOC.** This is the integration layer that wires
all other crates together.

## COMPLETE — Do Not Rewrite

### Server Infrastructure
- **build_router()** — axum Router with all routes (GET /api/genres, GET /ws,
  GET /ws/watcher, static serves).
- **create_server() / serve_with_listener()** — server lifecycle with graceful shutdown.
- **AppState** — shared application state (Arc-wrapped). Integrates GameService,
  broadcast channels, connections, persistence, render queue, subject extractor,
  beat filter, multiplayer sessions, audio mixer, music director.

### WebSocket Session Management
- **handle_ws_connection()** — reader/writer split with 3-way broadcast (direct,
  session, binary).
- **handle_watcher_connection()** — read-only telemetry stream for GM panel.
- **Session state machine** — AwaitingConnect -> Creating -> Playing.
- **ProcessingGuard** — RAII guard preventing concurrent actions per player.

### Message Dispatch
Now split into `dispatch/` subdirectory with focused modules:
- **dispatch/mod.rs** — main dispatch routing (by message type)
- **dispatch/connect.rs** — genre/world binding, session initialization
- **dispatch/audio.rs** — audio system dispatch (music, SFX, ambience)
- **dispatch/combat.rs** — combat dispatch
- **dispatch/render.rs** — render integration
- **dispatch/prompt.rs** — prompt building
- **dispatch/session_sync.rs** — session synchronization
- **dispatch/slash.rs** — slash command dispatch
- **dispatch/state_mutations.rs** — state mutations
- **dispatch/tropes.rs** — trope system dispatch

### Multiplayer
- **SharedGameSession** — `shared_session.rs` (215 LOC) — world-level state shared
  across players (genre:world keyed). Has sync_to_locals() / sync_from_locals()
  for state synchronization between session and dispatch loop.

### Render Integration
- **spawn_image_broadcaster()** — `render_integration.rs` (125 LOC) — async task
  converting render results to IMAGE messages. Path rewriting, tier/scene_type
  mapping, empty URL guards.

### Telemetry
- **WatcherEvent** — structured telemetry for GM panel. Types: AgentSpanOpen/Close,
  StateTransition, ValidationWarning, SubsystemExerciseSummary, CoverageGap.
- **NPC registry** — update_npc_registry() extracts NPC names via regex patterns
  from narration text.

## PARTIAL — Known Gaps

- **Perception rewriting** — infrastructure wired in SharedGameSession but strategy
  is RED phase / stub.
- **Turn barrier integration** — types present but engagement with dispatch unclear.
- **2 small TODOs** in dispatch_player_action: combatant name extraction, active
  dialogue NPC parsing. Non-blocking.

## Architectural Note

Combat is a cross-cutting concern spanning this crate and sidequest-game. The
routing (intent classification -> agent dispatch -> state patches -> broadcast)
all happens inside dispatch_player_action(). Future refactoring should extract
combat orchestration into its own module or service.

## Important

- **dispatch_player_action() is 1,950 lines.** If you need to modify it, read the
  full function first. Do not guess where things happen — line numbers shift with
  every PR.
- The server depends on ALL other crates. Check their CLAUDE.md files for what's
  available before adding new types.
