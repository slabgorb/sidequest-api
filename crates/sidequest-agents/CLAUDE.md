# sidequest-agents — Feature Inventory

LLM agent orchestration via Claude CLI subprocess. **~7,600 LOC across 30+ modules.**
This crate handles intent classification, agent dispatch, prompt composition, and
response extraction.

## COMPLETE — Do Not Rewrite

### Core Architecture
- **Agent trait** — `agent.rs` (41 LOC) — name(), system_prompt(), build_context().
  All agents implement this.
- **ClaudeClient** — `client.rs` (317 LOC) — subprocess wrapper (`claude -p`). Has
  timeout (default 120s), builder pattern, fallback semantics (ADR-005: degraded
  response on timeout, not crash).
- **Orchestrator** — `orchestrator.rs` (222 LOC) — GameService trait + state machine.
  Intent routing -> agent dispatch -> patch application -> result. This is the
  main game loop entry point from the server.
- **ContextBuilder** — `context_builder.rs` (93 LOC) — zone-ordered prompt composition
  (Primacy, Situational, Anchoring, Grounding).
- **Extractor** — `extractor.rs` (146 LOC) — response parsing/extraction from Claude output.

### Agent Implementations (4 COMPLETE)
- **Narrator** — `narrator.rs` (63 LOC) — exploration, description, story progression.
  OCEAN personality injection (story 10-4). Knowledge extraction (story 9-4).
  Footnote parsing (story 9-11).
- **CreatureSmith** — `creature_smith.rs` (66 LOC) — combat resolution, tactical encounters.
  Routed to when TurnContext.in_combat is true.
- **Ensemble** — `ensemble.rs` (66 LOC) — NPC dialogue & interaction.
- **Dialectician** — `dialectician.rs` (66 LOC) — chase sequences (pursuit, escape, negotiation).

### Intent Classification
- **IntentRouter** — `intent_router.rs` (251 LOC) — 2-tier classification (ADR-032):
  1. State override (in_combat -> Combat, in_chase -> Chase)
  2. Keyword fallback (synchronous, no LLM call)
  Intent enum: Combat, Dialogue, Exploration, Examine, Meta, Chase.

### Prompt Framework (story 3-1)
- **PromptSection / AttentionZone** — `prompt_framework/` (1,484 LOC total) —
  zone-ordered prompt assembly with telemetry. Zones: Primacy, Situational,
  Anchoring, Grounding. Fully tested.
- **Soul** — `prompt_framework/soul.rs` (131 LOC) — character personality embedding
  in prompts.

### Support Systems
- **TurnRecord** — `turn_record.rs` (150 LOC) — turn history & telemetry (story 3-2).
- **ExerciseTracker** — `exercise_tracker.rs` (120 LOC) — agent invocation history (story 3-5).
- **EntityReference** — `entity_reference.rs` (200 LOC) — NPC/entity ID resolution (story 3-4).
- **PatchLegality** — `patch_legality.rs` (202 LOC) — validate patches before applying (story 3-3).
- **TropeAlignment** — `trope_alignment.rs` (134 LOC) — trope compatibility checking (story 3-8).
- **Footnotes** — `footnotes.rs` (38 LOC) — footnote extraction from narrator output.

## STUBS — Defined but NOT Implemented

These have Agent trait impls but do nothing useful. 49 LOC each = just scaffolding.

- **Resonator** — `resonator.rs` (49 LOC) — **STUB ONLY.** Python original is TWO
  separate components: `hook_refiner.py` (~150 LOC, LLM-assisted narrative hook
  polishing) + `perception_rewriter.py` (~190 LOC, per-player narration rewriting
  based on perception effects like blinded/charmed/dominated). The Rust stub
  combines both responsibilities but implements neither.
- **Troper** — `troper.rs` (49 LOC) — **STUB ONLY.** No discrete Troper agent exists
  in Python either — trope logic is distributed across `state.py` (lifecycle),
  `state_processor.py` (passive ticking), and `prompt_composer.py` (LLM context).
  The Rust version (per ADR-018) intends to consolidate these into one agent.
  NOTE: The TropeEngine in sidequest-game/trope.rs already handles ticking and
  escalation — the Troper agent's job would be LLM-driven trope activation and
  narrative beat injection, not the mechanical progression.
- **WorldBuilder** — `world_builder.rs` (49 LOC) — **STUB ONLY.** Python original
  at `game/world_builder.py` (~500 LOC) is a builder pattern that materializes
  dense GameState at specified maturity levels (FRESH/EARLY/MID/VETERAN) with
  NPCs, items, lore. Note: Python version is a builder class, NOT an LLM agent.
  The Rust world_materialization.rs in sidequest-game already handles the maturity
  model — check what remains for this agent to do.

Do NOT integrate with these stubs. They need full implementation first.

## Key Patterns

- **GameService trait**: the server calls `process_action()` — that's the entire interface
- **2-tier intent classification**: state override first, keyword fallback second (no LLM for classification)
- **Zone-ordered prompts**: Primacy (identity) -> Situational (state) -> Anchoring (rules) -> Grounding (history)
- **Subprocess model**: Claude CLI, not SDK — `claude -p` with JSON I/O
