# SideQuest API

Rust game engine for the SideQuest AI Narrator -- a tabletop RPG engine
where coordinated Claude agents run the game. Ported from Python
([sq-2](https://github.com/slabgorb/sq-2)).

Players connect via WebSocket. Seven Claude agents collaborate as the game
master: narrating scenes, building worlds, creating creatures, managing
dialogue, and routing player intent. The ML stack (image generation, TTS,
audio mixing) stays in Python as a sidecar daemon.

## Architecture

```
React Client (sidequest-ui)
    |                                |
    | WebSocket /ws                  | REST /api/*
    v                                v
+------------------------------------------------+
|  sidequest-server                              |
|  axum HTTP/WS, session lifecycle, CORS,        |
|  structured tracing, watcher telemetry         |
+------------------------+-----------------------+
                         | GameService trait
                         v
+------------------------------------------------+
|  sidequest-agents                              |
|  7 agents, orchestrator state machine,         |
|  ClaudeClient, prompt framework, JSON extract  |
+------------+-------------------+---------------+
             |                   |
             v                   v
+--------------------+  +--------------------+  +---------------------------+
| sidequest-game     |  | sidequest-genre    |  | sidequest-daemon-client   |
| GameSnapshot, NPCs,|  | YAML pack loader,  |  | Unix socket client for    |
| combat, chase,     |  | trope inheritance,  |  | Python media daemon       |
| persistence, turns |  | name gen, cache    |  | (JSON-RPC)                |
+---------+----------+  +---------+----------+  +---------------------------+
          |                       |
          +-----------+-----------+
                      v
          +--------------------+
          | sidequest-protocol |
          | GameMessage enum,  |
          | newtypes, sanitize |
          +--------------------+
```

## Crates

### sidequest-protocol -- COMPLETE

Communication protocol between UI and server. Defines the `GameMessage` enum,
typed payloads for every message type, input sanitization, and newtype wrappers
for domain IDs.

**Depends on:** nothing

### sidequest-genre -- COMPLETE

YAML genre pack loader and narrative models. Handles pack loading and
validation, trope inheritance resolution, name generation (Markov chains +
template blending), and a genre cache for hot-reloading packs at runtime.

**Depends on:** protocol

### sidequest-game -- 95% COMPLETE

Core game state engine. 26+ modules covering:

- **GameSnapshot** -- composable game state
- **Characters/NPCs** -- full model with inventory, abilities, disposition
- **Combat** -- classification, resolution, turn management
- **Chase** -- cinematic chase engine ([ADR-017](../docs/adr/017-cinematic-chase-engine.md))
- **Turn modes** -- exploration, combat, chase, character creation
- **Session lifecycle** -- start, save, load, resume
- **Persistence** -- SQLite via rusqlite
- **Character builder** -- state machine ([ADR-015](../docs/adr/015-character-builder-state-machine.md))
- **Tension tracker** -- dual-track model ([ADR-024](../docs/adr/024-dual-track-tension-model.md))
- **Beat filter, render queue, segmenter** -- pacing control ([ADR-025](../docs/adr/025-pacing-detection.md))

One module stubbed: `perception.rs` (RED phase, story 8-6).

**Depends on:** protocol, genre

### sidequest-agents -- COMPLETE

Claude CLI subprocess orchestration. All LLM calls go through
`claude -p` as a Tokio subprocess, never through the Anthropic SDK
([ADR-001](../docs/adr/001-claude-cli-only.md)).

**7 agents:** Narrator, WorldBuilder, CreatureSmith, Ensemble, Dialectician,
IntentRouter, Troper.

**Infrastructure:**
- Orchestrator -- `GameService` trait, agent state machine
- ClaudeClient -- subprocess management, timeout handling
- Prompt framework -- attention zones (EARLY/VALLEY/LATE) ([ADR-009](../docs/adr/009-attention-aware-prompt-zones.md))
- JSON extractor -- 3-tier fallback ([ADR-013](../docs/adr/013-lazy-json-extraction.md))
- Context builder, turn record telemetry, patch legality, entity tracking

**Depends on:** protocol, game

### sidequest-server -- COMPLETE

axum HTTP/WebSocket server. REST endpoints for save/load, character listing,
genre pack metadata. WebSocket at `/ws` for real-time game events. Includes
session lifecycle, CORS, structured tracing via `tracing-subscriber`,
a `/watcher` endpoint for telemetry ([ADR-031](../docs/adr/031-game-watcher-semantic-telemetry.md)),
and graceful shutdown.

**Depends on:** all other crates

### sidequest-daemon-client -- COMPLETE

Unix socket client for the Python media daemon
([sidequest-daemon](https://github.com/slabgorb/sidequest-daemon)).
JSON-RPC protocol for image generation, TTS, and audio requests.
Typed request/response structs with error handling.

**Depends on:** protocol

## Build and Test

```bash
cargo build                           # Build all 6 crates
cargo test                            # Run all tests (182 test files)
cargo clippy -- -D warnings           # Lint
cargo fmt -- --check                  # Format check
cargo run -p sidequest-server         # Run the server
```

Requires Rust 1.80+ (edition 2021). See `rust-toolchain.toml` for the
pinned version.

## Key Design Decisions

| Decision | Rationale | ADR |
|----------|-----------|-----|
| Claude CLI only | Subprocess calls (`claude -p`), not the SDK. Simpler auth, observable, debuggable. | [001](../docs/adr/001-claude-cli-only.md) |
| GameService facade | Server depends on a trait, never on game internals. Keeps axum layer thin and testable. | -- |
| JSON delta patches | Agents emit state patches, not full state replacements. Bandwidth-efficient, auditable. | [011](../docs/adr/011-world-state-json-patches.md) |
| Genre packs as YAML | Runtime-swappable narrative configuration. Any genre, any setting. | [003](../docs/adr/003-genre-pack-architecture.md) |
| Graceful degradation | Agent timeouts produce degraded responses instead of errors. The game never crashes. | [006](../docs/adr/006-graceful-degradation.md) |
| Attention-aware prompts | Prompt zones (EARLY/VALLEY/LATE) place critical context where the LLM pays attention. | [009](../docs/adr/009-attention-aware-prompt-zones.md) |
| 3-tier JSON extraction | Regex, then partial parse, then re-prompt. Handles malformed LLM output. | [013](../docs/adr/013-lazy-json-extraction.md) |
| Story-driven testing | Tests named by feature story (e.g., `combat_classification_story_5_2_tests.rs`). | -- |

All ADRs live in the orchestrator at [`docs/adr/`](../docs/adr/).

## Python Equivalents

For developers coming from the Python codebase:

| Python (sq-2) | Rust (this repo) |
|----------------|-----------------|
| Pydantic models | serde structs |
| asyncio | tokio |
| aiohttp | axum |
| pyyaml | serde_yaml |
| sqlite3 | rusqlite |
| `subprocess.run(["claude", "-p", ...])` | `tokio::process::Command` |

## Related Repos

- [orc-quest](https://github.com/slabgorb/orc-quest) -- Orchestrator (sprint tracking, ADRs, genre packs)
- [sidequest-ui](https://github.com/slabgorb/sidequest-ui) -- React/TypeScript game client
- [sidequest-daemon](https://github.com/slabgorb/sidequest-daemon) -- Python media services (image gen, TTS, audio)

## Git Workflow

Default branch: `develop` (gitflow). Feature branches: `feat/{description}`.
PRs target `develop`.
