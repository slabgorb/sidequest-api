# sidequest-server — Feature Inventory

axum HTTP/WebSocket server. **~16,000 LOC.** This is the integration layer that wires
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
Split into `dispatch/` subdirectory with 22 focused modules (~10,800 LOC total):
- **dispatch/connect.rs** (2,692 LOC) — genre/world binding, session initialization
- **dispatch/mod.rs** (2,529 LOC) — main dispatch routing (by message type)
- **dispatch/prompt.rs** (950 LOC) — prompt context building (`build_prompt_context()`)
- **dispatch/audio.rs** (589 LOC) — audio system dispatch (music, SFX, ambience, OTEL)
- **dispatch/state_mutations.rs** (436 LOC) — state mutation application (combat engage/disengage, HP, turn mode)
- **dispatch/render.rs** (386 LOC) — render integration with SceneRelevanceValidator
- **dispatch/response.rs** (369 LOC) — response formatting and delivery
- **dispatch/slash.rs** (296 LOC) — slash command dispatch
- **dispatch/npc_registry.rs** (287 LOC) — NPC name extraction, OCEAN shift pipeline
- **dispatch/chargen_summary.rs** (266 LOC) — character generation summary
- **dispatch/barrier.rs** (246 LOC) — turn barrier coordination
- **dispatch/session_sync.rs** (233 LOC) — session synchronization, perception rewriting
- **dispatch/lore_embed_worker.rs** (233 LOC) — lore embedding worker
- **dispatch/beat.rs** (194 LOC) — beat dispatch
- **dispatch/lore_sync.rs** (174 LOC) — lore synchronization
- **dispatch/tropes.rs** (171 LOC) — trope system dispatch
- **dispatch/persistence.rs** (171 LOC) — save/load dispatch
- **dispatch/pregen.rs** (162 LOC) — pre-generation dispatch
- **dispatch/telemetry.rs** (158 LOC) — telemetry dispatch
- **dispatch/aside.rs** (111 LOC) — aside/whisper dispatch
- **dispatch/catch_up.rs** (101 LOC) — catch-up narration dispatch
- **dispatch/patching.rs** (56 LOC) — state patch application

### Multiplayer
- **SharedGameSession** — `shared_session.rs` (455 LOC) — world-level state shared
  across players (genre:world keyed). Has sync_to_locals() / sync_from_locals()
  for state synchronization between session and dispatch loop. Includes perception
  filter storage for per-player narration variants.

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

## Important

- The server depends on ALL other crates. Check their CLAUDE.md files for what's
  available before adding new types.
