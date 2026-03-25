# CLAUDE.md — SideQuest API (Rust)

Rust game engine for the SideQuest AI Narrator. Ported from Python (sq-2).

## CRITICAL: Personal Project

- **No Jira.** Never create or reference Jira tickets.
- **All repos under `slabgorb` on GitHub.** Not 1898. Ever.

## Build Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt                # Format
cargo run                # Run the server
```

## Architecture

Port of the Python game engine with these Rust equivalents:
- Pydantic models → serde structs
- asyncio → tokio
- Rich TUI → ratatui (eventually)
- aiohttp → axum
- pyyaml → serde_yaml
- sqlite3 → rusqlite
- claude CLI subprocess → tokio::process::Command

The ML stack (image gen, TTS, audio) stays in Python as a sidecar daemon.

## Git Workflow

- Branch strategy: gitflow
- Default branch: develop
- Feature branches: `feat/{description}`
- PRs target: develop
