# sidequest-agents

Claude CLI subprocess orchestration, prompt composition, and response parsing.

All LLM calls go through `claude -p` as a subprocess — never through the
Anthropic SDK. See [ADR-001](../../../docs/adr/001-claude-cli-only.md).

## What's in here

- **`orchestrator`** — `GameService` trait and `Orchestrator` implementation that
  routes player actions to the right agent
- **`agents/`** — Individual agent implementations:
  - `narrator` — Narrative generation
  - `creature_smith` — Character creation
  - `world_builder` — World generation
  - `intent_router` — Player intent classification
  - `troper` — Narrative trope application
  - `ensemble` — Multi-agent coordination
- **`client`** — Claude CLI wrapper (`tokio::process::Command`)
- **`prompt_framework`** — Three-tier prompt taxonomy (Critical / Firm / Coherence)
- **`context_builder`** — Assembles prompt context from game state
- **`extractor`** — JSON extraction from Claude responses (3-tier fallback)
- **`patches`** — Applies agent-emitted state patches
- **`format_helpers`** — Output formatting utilities

## Key design notes

- The `GameService` trait is the boundary between server and game logic —
  the server never touches game internals directly
- Prompts use attention-aware zones (EARLY/VALLEY/LATE) to position content
  where the model pays most attention ([ADR-009](../../../docs/adr/009-attention-aware-prompt-zones.md))
- JSON extraction uses a 3-tier fallback: direct parse → regex extract → re-prompt
  ([ADR-013](../../../docs/adr/013-lazy-json-extraction.md))
