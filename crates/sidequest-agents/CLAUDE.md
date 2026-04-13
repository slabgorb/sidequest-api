# sidequest-agents — Feature Inventory

LLM agent orchestration via Claude CLI subprocess. **~10,300 LOC across 52 modules.**
This crate handles intent classification, agent dispatch, prompt composition,
response extraction, and post-narration tool result assembly.

## COMPLETE — Do Not Rewrite

### Core Architecture
- **Agent trait** — `agent.rs` (41 LOC) — name(), system_prompt(), build_context().
  All agents implement this.
- **ClaudeClient** — `client.rs` (317 LOC) — subprocess wrapper (`claude -p`). Has
  timeout (default 120s), builder pattern, fallback semantics (ADR-005: degraded
  response on timeout, not crash). Supports `--session-id` (new) and `--resume`
  (delta) for persistent Claude sessions.
- **Orchestrator** — `orchestrator.rs` (1,445 LOC) — GameService trait + state machine.
  Intent routing → agent dispatch → tiered prompt assembly → patch application → result.
  This is the main game loop entry point from the server.
  Key types: ActionResult (20+ fields), NarratorPromptTier (Full/Delta per ADR-066),
  TurnContext, TurnResult, AgentKind enum.
  `build_narrator_prompt_tiered()` assembles ~30 prompt sections across 5 attention zones.
- **ContextBuilder** — `context_builder.rs` (172 LOC) — zone-ordered prompt composition.
  Zones: **Primacy, Early, Valley, Late, Recency** (highest to lowest attention).
  `compose()` sorts sections by zone before joining.
- **Extractor** — `extractor.rs` (146 LOC) — response parsing/extraction from Claude output.

### Agent Implementations (7 COMPLETE)
- **Narrator** — `narrator.rs` (447 LOC) — exploration, description, story progression.
  Contains complete game_patch JSON schema (~210 lines defining ALL fields the narrator
  can emit). Sections: NARRATOR_IDENTITY, NARRATOR_CONSTRAINTS, NARRATOR_AGENCY,
  NARRATOR_CONSEQUENCES, NARRATOR_OUTPUT_ONLY, NARRATOR_OUTPUT_STYLE,
  NARRATOR_REFERRAL_RULE, NARRATOR_COMBAT_RULES, NARRATOR_CHASE_RULES,
  NARRATOR_DIALOGUE_RULES. Methods: build_output_format(), build_combat_context(),
  build_chase_context(), build_dialogue_context().
- **CreatureSmith** — `creature_smith.rs` (66 LOC) — combat resolution, tactical encounters.
  Routed to when TurnContext.in_combat is true.
- **Ensemble** — `ensemble.rs` (66 LOC) — NPC dialogue & interaction.
- **Dialectician** — `dialectician.rs` (66 LOC) — chase sequences (pursuit, escape, negotiation).
- **Resonator** — `resonator.rs` (479 LOC) — TWO responsibilities: hook refinement
  (narrative hook polishing) + perception rewriting (per-player narration based on
  status effects). Implements `ClaudeRewriteStrategy` and `FullContextRewriteStrategy`
  for the `RewriteStrategy` trait from sidequest-game/perception.rs. Wired into
  `dispatch/session_sync.rs`.
- **Troper** — `troper.rs` (720 LOC) — trope beat injection into narrator context.
  Translates mechanical TropeEngine escalation beats (from sidequest-game/trope.rs)
  into narrative instructions for the narrator. Full prompt framework with zone-ordered
  sections, active/dormant/completed trope classification.
- **WorldBuilder** — `world_builder.rs` (497 LOC) — progressive world materialization
  based on campaign maturity (Fresh/Early/Mid/Veteran). Generates locations, NPCs,
  lore, faction developments scaled to maturity tier. Full prompt framework with
  zone-ordered sections.

### Intent Classification
- **IntentRouter** — `intent_router.rs` (251 LOC) — state-override classification (ADR-067):
  1. State override (in_combat → Combat, in_chase → Chase)
  2. Default: Exploration (narrator handles everything)
  No keyword matching — ADR-067 eliminated keyword fallback. Combat/chase routing
  is purely state-driven. The narrator is responsible for emitting in_combat/in_chase
  in game_patch to transition states.
  Intent enum: Combat, Dialogue, Exploration, Examine, Meta, Chase.

### Prompt Framework (story 3-1)
- **PromptSection / AttentionZone** — `prompt_framework/` (1,484 LOC total) —
  zone-ordered prompt assembly with telemetry. Zones: Primacy, Early, Valley,
  Late, Recency. Per-zone token estimates emitted via OTEL for Prompt Inspector.
- **Soul** — `prompt_framework/soul.rs` (131 LOC) — SOUL.md principles embedded in
  prompts, filtered per agent via `<agents>` tags.
- **LoreFilter** — `lore_filter.rs` — graph-distance-based world lore filtering
  for prompt injection.

### Post-Narration Tools (ADR-057/059)
- **tools/** — mechanical state change handlers that run alongside narration.
  The narrator does NOT call these tools — they are invoked server-side.
  `assemble_turn` merges tool results with narration (tool values win).
  Modules: `assemble_turn`, `item_acquire`, `personality_event`, `play_sfx`,
  `quest_update`, `resource_change`, `scene_render`, `set_intent`, `set_mood`,
  `lore_mark`, `merchant_transact`, `tool_call_parser`. Plus `preprocessors.rs`
  for input preprocessing.

### Support Systems
- **TurnRecord** — `turn_record.rs` (150 LOC) — turn history & telemetry (story 3-2).
- **ExerciseTracker** — `exercise_tracker.rs` (120 LOC) — agent invocation history (story 3-5).
- **EntityReference** — `entity_reference.rs` (200 LOC) — NPC/entity ID resolution (story 3-4).
- **PatchLegality** — `patch_legality.rs` (202 LOC) — validate patches before applying (story 3-3).
- **TropeAlignment** — `trope_alignment.rs` (134 LOC) — trope compatibility checking (story 3-8).
- **Footnotes** — `footnotes.rs` (38 LOC) — footnote extraction from narrator output.
- **ContinuityValidator** — `continuity_validator.rs` — continuity checking across turns.
- **InventoryExtractor** — item extraction from narration.

## Key Patterns

- **GameService trait**: the server calls `process_action()` — that's the entire interface
- **State-override intent classification**: in_combat → Combat, in_chase → Chase, default → Exploration (no keyword matching per ADR-067, no LLM)
- **Zone-ordered prompts**: Primacy (identity) → Early (rules) → Valley (state) → Late (format) → Recency (action)
- **Tiered prompts (ADR-066)**: Full tier (first turn, ~15KB system prompt) vs Delta tier (resumed session, dynamic state only). `narrator_output_only` (game_patch schema) re-sent every turn.
- **Subprocess model**: Claude CLI, not SDK — `claude -p` with `--session-id` / `--resume` for persistent sessions
