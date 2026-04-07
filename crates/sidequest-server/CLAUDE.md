# sidequest-server — Feature Inventory

axum HTTP/WebSocket server. **~11,700 LOC.** This is the integration layer that wires
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
- **Session state machine** — AwaitingConnect → Creating → Playing.
- **ProcessingGuard** — RAII guard preventing concurrent actions per player.

### Message Dispatch
Split into `dispatch/` subdirectory with 12 focused modules (~7,700 LOC total):
- **dispatch/mod.rs** (3,022 LOC) — main dispatch routing (by message type)
- **dispatch/connect.rs** (1,689 LOC) — genre/world binding, session initialization
- **dispatch/prompt.rs** (803 LOC) — prompt context building (`build_prompt_context()`)
- **dispatch/state_mutations.rs** (800 LOC) — state mutation application (combat engage/disengage, HP, turn mode)
- **dispatch/audio.rs** (338 LOC) — audio system dispatch (music, SFX, ambience, OTEL)
- **dispatch/session_sync.rs** (225 LOC) — session synchronization
- **dispatch/render.rs** (163 LOC) — render integration with SceneRelevanceValidator
- **dispatch/pregen.rs** (157 LOC) — pre-generation dispatch
- **dispatch/tropes.rs** (147 LOC) — trope system dispatch
- **dispatch/combat.rs** (121 LOC) — combat event broadcast, status effect ticking
- **dispatch/slash.rs** (111 LOC) — slash command dispatch
- **dispatch/catch_up.rs** (101 LOC) — catch-up narration dispatch

### Multiplayer
- **SharedGameSession** — `shared_session.rs` (215 LOC) — world-level state shared
  across players (genre:world keyed). Has sync_to_locals() / sync_from_locals()
  for state synchronization between session and dispatch loop.

### Render Integration
- **spawn_image_broadcaster_with_throttle()** — `render_integration.rs` — async task
  converting render results to IMAGE messages. Image pacing throttle (30s solo,
  60s multiplayer). Path rewriting, tier/scene_type mapping, empty URL guards,
  handout classification.

### Telemetry
- **WatcherEvent** — structured telemetry for GM panel. Types: AgentSpanOpen/Close,
  StateTransition, ValidationWarning, SubsystemExerciseSummary, CoverageGap.
- **NPC registry** — update_npc_registry() extracts NPC names via regex patterns
  from narration text.

## PARTIAL — Known Gaps

- **Perception rewriting** — infrastructure wired in SharedGameSession but strategy
  is RED phase / stub.

## Important

- The server depends on ALL other crates. Check their CLAUDE.md files for what's
  available before adding new types.
