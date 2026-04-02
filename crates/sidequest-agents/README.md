# sidequest-agents

Claude CLI subprocess orchestration, prompt composition, and response parsing.

All LLM calls go through `claude -p` as a subprocess — never through the
Anthropic SDK. See [ADR-001](../../../docs/adr/001-claude-cli-only.md).

## What's in here

- **`orchestrator`** — `GameService` trait and `Orchestrator` implementation that
  routes player actions to the right agent
- **`agents/`** — Individual agent implementations:
  - `narrator` — Exploration, story progression, OCEAN injection, knowledge extraction
  - `creature_smith` — Combat resolution, tactical encounters
  - `ensemble` — NPC dialogue and interaction
  - `dialectician` — Chase sequences (pursuit, escape, negotiation)
  - `intent_router` — 2-tier player intent classification
  - `resonator` — Hook refiner + perception rewriter (scaffolding)
  - `troper` — Trope logic orchestration (scaffolding)
  - `world_builder` — World materialization at maturity levels (scaffolding)
- **`tools/`** — Sidecar tool definitions and parsers (assemble_turn, scene_render, quest_update, etc.)
- **`client`** — Claude CLI wrapper (`tokio::process::Command`)
- **`prompt_framework`** — Zone-ordered prompt assembly (Primacy / Situational / Anchoring / Grounding)
- **`context_builder`** — Assembles prompt context from game state
- **`extractor`** — JSON extraction from Claude responses (3-tier fallback)
- **`patches`** — Applies agent-emitted state patches

## Key design notes

- The `GameService` trait is the boundary between server and game logic —
  the server never touches game internals directly
- Prompts use attention-aware zones (EARLY/VALLEY/LATE) to position content
  where the model pays most attention ([ADR-009](../../../docs/adr/009-attention-aware-prompt-zones.md))
- JSON extraction uses a 3-tier fallback: direct parse → regex extract → re-prompt
  ([ADR-013](../../../docs/adr/013-lazy-json-extraction.md))
