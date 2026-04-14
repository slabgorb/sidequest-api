# sidequest-agents

Claude CLI subprocess orchestration, prompt composition, and response parsing.

All LLM calls go through `claude -p` as a subprocess — never through the
Anthropic SDK. See **ADR-001** in the orchestrator repo (`orc-quest/docs/adr/`).

## What's in here

- **`orchestrator`** — `GameService` trait and `Orchestrator` implementation that
  routes player actions to the right agent and composes tiered prompts.
- **`agents/`** — Individual agent implementations:
  - `narrator` — Exploration, combat, chase, dialogue. Per ADR-067 the narrator
    is the unified agent — combat, dialogue, and chase were absorbed into it
    (no more separate `creature_smith` / `ensemble` / `dialectician` files).
  - `intent_router` — State-override classification: `in_combat` → Combat,
    `in_chase` → Chase, otherwise Exploration. No keyword matching (ADR-067).
  - `resonator` — Hook refiner and per-player perception rewriter
  - `troper` — Trope beat injection into narrator context
  - `world_builder` — Progressive world materialization by campaign maturity
- **`tools/`** — Sidecar tool definitions and parsers (`assemble_turn`,
  `item_acquire`, `personality_event`, `play_sfx`, `quest_update`,
  `resource_change`, `scene_render`, `set_intent`, `set_mood`, `lore_mark`,
  `merchant_transact`, `tool_call_parser`)
- **`client`** — Claude CLI wrapper (`tokio::process::Command`). Supports
  `--session-id` (new) and `--resume` (delta) for persistent sessions (ADR-066).
- **`prompt_framework`** — Zone-ordered prompt assembly. Zones:
  `Primacy → Early → Valley → Late → Recency` (highest-attention to lowest).
- **`context_builder`** — Sorts and joins `PromptSection`s by zone
- **`patches` / `patch_legality`** — Narrator-emitted state patch application
  and pre-apply validation
- **`lore_filter`** — Graph-distance-based world lore filtering
- **`inventory_extractor` / `entity_reference` / `continuity_validator` /
  `footnotes` / `turn_record` / `exercise_tracker`** — Support systems for
  turn post-processing

## Key design notes

- The `GameService` trait is the boundary between server and game logic —
  the server never touches game internals directly.
- Prompts use attention-aware zones (**Primacy / Early / Valley / Late /
  Recency**) to position content where the model pays most attention
  (ADR-009, in `orc-quest/docs/adr/`).
- Intent routing is **state-driven only** (ADR-067). Combat and chase are
  reached by the narrator setting `in_combat` / `in_chase` in its game_patch;
  there is no keyword fallback path.
- Tiered prompts (ADR-066): full ~15KB system prompt on first turn, delta
  tier on resumed sessions with dynamic state only. `narrator_output_only`
  (the `game_patch` schema) is re-sent every turn regardless of tier.

> ADRs live in the orchestrator repo at `orc-quest/docs/adr/`. They are
> **not** checked into this repo — links in this file are symbolic.
