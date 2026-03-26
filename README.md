# SideQuest API

Rust game engine for the SideQuest AI Narrator. Ported from Python ([sq-2](https://github.com/slabgorb/sq-2)).

An AI dungeon master that runs tabletop-style RPGs in any genre, powered by
coordinated Claude agents calling the `claude` CLI as subprocesses.

## Architecture

```
React Client (sidequest-ui)
    │ WebSocket /ws              │ REST /api/*
    ▼                            ▼
┌────────────────────────────────────────────┐
│  sidequest-server  (axum + WebSocket)      │
│  Routes, CORS, session lifecycle           │
└────────────────┬───────────────────────────┘
                 │ GameService trait
                 ▼
┌────────────────────────────────────────────┐
│  sidequest-agents  (orchestrator + agents) │
│  Claude CLI subprocesses, prompt composer  │
└────────────────┬───────────────────────────┘
                 │
      ┌──────────┴──────────┐
      ▼                     ▼
┌──────────────┐  ┌──────────────────┐
│sidequest-game│  │ sidequest-genre  │
│ State, combat│  │ YAML pack loader │
│ chase, NPCs  │  │ models, cache    │
└──────┬───────┘  └────────┬─────────┘
       │                   │
       └─────────┬─────────┘
                 ▼
       ┌──────────────────┐
       │sidequest-protocol│
       │ GameMessage enum │
       │ newtypes, sanitize│
       └──────────────────┘
```

## Crates

| Crate | Purpose | Depends on |
|-------|---------|------------|
| `sidequest-protocol` | `GameMessage` enum, typed payloads, input sanitization | — |
| `sidequest-genre` | YAML genre pack loader, models, validation, trope inheritance | protocol |
| `sidequest-game` | Game state, characters, combat, chase, inventory, persistence | protocol, genre |
| `sidequest-agents` | Claude CLI subprocess orchestration, prompt framework, JSON extraction | protocol, game |
| `sidequest-server` | axum HTTP/WebSocket server, session lifecycle, `GameService` facade | all above |

## Quick Start

```bash
# Build
cargo build

# Run tests (490 passing)
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Run the server
cargo run
```

## Key Design Decisions

- **Claude CLI only** — All LLM calls go through `claude -p` as a subprocess
  (`tokio::process::Command`), never through the Anthropic SDK.
  See [ADR-001](../docs/adr/001-claude-cli-only.md).

- **GameService facade** — The server depends on a trait, not on game internals.
  This keeps the axum layer thin and testable.

- **Genre packs are YAML** — Swappable narrative configuration loaded at runtime.
  See [ADR-003](../docs/adr/003-genre-pack-architecture.md).

- **Agents emit JSON patches** — State updates are deltas, not full replacements.
  See [ADR-011](../docs/adr/011-world-state-json-patches.md).

## Python Equivalents

| Python (sq-2) | Rust (this repo) |
|----------------|-----------------|
| Pydantic models | serde structs |
| asyncio | tokio |
| aiohttp | axum |
| pyyaml | serde_yaml |
| sqlite3 | rusqlite |
| `subprocess.run(["claude", "-p", ...])` | `tokio::process::Command` |

The ML stack (image gen, TTS, audio) stays in Python as
[sidequest-daemon](https://github.com/slabgorb/sidequest-daemon).

## Related Repos

- [orc-quest](https://github.com/slabgorb/orc-quest) — Orchestrator (sprint tracking, ADRs, genre packs)
- [sidequest-ui](https://github.com/slabgorb/sidequest-ui) — React/TypeScript game client
- [sidequest-daemon](https://github.com/slabgorb/sidequest-daemon) — Python media services (image gen, TTS, audio)

## Git Workflow

- Default branch: `develop`
- Feature branches: `feat/{description}`
- PRs target `develop`
