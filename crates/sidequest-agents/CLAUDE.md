# sidequest-agents — Feature Inventory

LLM agent orchestration via Claude CLI subprocess. **~8,100 LOC across 38 modules.**
This crate handles intent classification, agent dispatch, prompt composition,
response extraction, and sidecar tool parsing.

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

### Sidecar Tools
- **tools/** — tool definitions and sidecar parsers for narrator tool calls:
  `assemble_turn`, `personality_event`, `play_sfx`, `quest_update`,
  `resource_change`, `scene_render`, `set_intent`, `set_mood`, `tool_call_parser`.
  Plus `preprocessors.rs` for input preprocessing.

### Support Systems
- **TurnRecord** — `turn_record.rs` (150 LOC) — turn history & telemetry (story 3-2).
- **ExerciseTracker** — `exercise_tracker.rs` (120 LOC) — agent invocation history (story 3-5).
- **EntityReference** — `entity_reference.rs` (200 LOC) — NPC/entity ID resolution (story 3-4).
- **PatchLegality** — `patch_legality.rs` (202 LOC) — validate patches before applying (story 3-3).
- **TropeAlignment** — `trope_alignment.rs` (134 LOC) — trope compatibility checking (story 3-8).
- **Footnotes** — `footnotes.rs` (38 LOC) — footnote extraction from narrator output.
- **ContinuityValidator** — `continuity_validator.rs` — continuity checking across turns.

## NEEDS FULL IMPLEMENTATION — Not Stubs

These have Agent trait impls but are minimal scaffolding (49 LOC each). All three
are fully implemented in the Python codebase and in sidequest-game's Rust engine,
but the agent-level LLM orchestration is not yet ported.

- **Resonator** — `resonator.rs` (372+ LOC in Python) — TWO responsibilities:
  `hook_refiner.py` (~150 LOC, LLM-assisted narrative hook polishing) +
  `perception_rewriter.py` (~190 LOC, per-player narration rewriting based on
  perception effects like blinded/charmed/dominated). The Rust agent needs to
  orchestrate both via Claude CLI.
- **Troper** — `troper.rs` (728+ LOC across Python modules) — trope logic distributed
  across `state.py` (lifecycle), `state_processor.py` (passive ticking), and
  `prompt_composer.py` (LLM context). The TropeEngine in sidequest-game/trope.rs
  handles ticking and escalation — the Troper agent's job is LLM-driven trope
  activation and narrative beat injection.
- **WorldBuilder** — `world_builder.rs` (500+ LOC in Python) — materializes dense
  GameState at specified maturity levels (FRESH/EARLY/MID/VETERAN). The Rust
  world_materialization.rs in sidequest-game handles the maturity model — check
  what remains for this agent.

These need full implementation, not integration with their current scaffolding.

## Key Patterns

- **GameService trait**: the server calls `process_action()` — that's the entire interface
- **2-tier intent classification**: state override first, keyword fallback second (no LLM for classification)
- **Zone-ordered prompts**: Primacy (identity) -> Situational (state) -> Anchoring (rules) -> Grounding (history)
- **Subprocess model**: Claude CLI, not SDK — `claude -p` with JSON I/O
