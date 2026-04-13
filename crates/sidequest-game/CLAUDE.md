# sidequest-game — Feature Inventory

The core game engine crate. **~29,300 LOC across 70 modules.** Almost everything
you're looking for is already here. Read this before writing any code.

## COMPLETE — Do Not Rewrite

These systems are fully implemented and production-ready. Do NOT stub, recreate,
or rewrite them. Use the existing types and functions.

### Core Game State
- **GameSnapshot** — `state.rs` (919 LOC) — master state composition struct.
  Captures characters, NPCs, location, `encounter`, tropes, atmosphere, lore,
  stakes, turns. Typed patch types: `WorldStatePatch`, `NpcPatch`. (Combat/chase
  are unified under `encounter: Option<StructuredEncounter>` — see ADR-033. The
  old `CombatPatch`/`ChasePatch` types were removed with story 16-2.)
- **Character** — `character.rs` (170 LOC) — player character with narrative identity
  (backstory, hooks) + mechanical stats. Embeds CreatureCore via composition.
  Implements Combatant trait.
- **Npc** — `npc.rs` (297 LOC) — non-player character with disposition-based attitude
  system (ADR-020). Identity-locked fields (pronouns, appearance). OCEAN personality
  support (story 10-1). merge_patch() protects identity fields.
- **CreatureCore** — `creature_core.rs` (129 LOC) — shared abstraction between
  Character and Npc. DRY composition type (story 1-13). Both embed this.
- **CharacterBuilder** — `builder.rs` (903 LOC) — multi-phase state machine for
  character creation. Loads CharCreationScene from genre pack, produces Character.

### Encounters & Turn Sequencing
- **StructuredEncounter** — `encounter.rs` (617 LOC) — universal encounter model
  (ADR-033 confrontation engine). Replaces the old split `CombatState`/`ChaseState`.
  One string-keyed `encounter_type` ("combat", "chase", "standoff", "negotiation"),
  one `EncounterMetric` (HP descending for combat, separation ascending for chase,
  etc.), one `SecondaryStats` block, shared `EncounterActor` list with string roles,
  shared phase/beat tracking. Constructors: `::combat(combatants, hp)`, `::chase(
  escape_threshold, rig_type, goal)`. Stored at `GameSnapshot.encounter`.
- **TurnManager** — `turn.rs` (165 LOC) — round counter, phase tracking, input
  barrier. TurnPhase: InputCollection -> IntentRouting -> AgentExecution -> StatePatch -> Broadcast.
- **TurnMode** — `turn_mode.rs` (79 LOC) — FreePlay / Structured / Cinematic state
  machine. Drives barrier behavior for multiplayer.
- **TurnBarrier** — `barrier.rs` (536 LOC) — concurrent turn coordination with
  adaptive timeout. Arc-wrapped with Mutex + Notify.
- **TurnReminder** — `turn_reminder.rs` (161 LOC) — idle player detection with
  mode-aware checks.
- **Combatant trait** — `combatant.rs` (153 LOC) — shared combat interface. Implemented
  by Character, Npc, CreatureCore.
- **HP clamping** — `hp.rs` (109 LOC) — pure function, fixes Python overflow bug.

### Multiplayer
- **MultiplayerSession** — `multiplayer.rs` (312 LOC) — player->Character map, turn
  collection, force_resolve_turn() with "hesitates" fallback. Max 6 players.
- **GuestNpc** — `guest_npc.rs` (205 LOC) — guest players control NPCs with limited
  agency (Dialogue, Movement, Examine only by default).

### Narrative & Audio
- **TensionTracker** — `tension_tracker.rs` (780 LOC) — dual-track pacing model
  (action + stakes tension). Produces PacingHint for narrator prompt injection.
- **MusicDirector** — `music_director.rs` (667 LOC) — mood classification from
  narration, track selection from genre pack. Mood enum: Combat, Exploration,
  Tension, Triumph, Sorrow, Mystery, Calm.
- **AudioMixer** — `audio_mixer.rs` — 3-channel cue-driven mixer (Music, SFX,
  Ambience). `apply_cue()` routes AudioCue events; `set_ambience()` swaps the
  ambience bed. The old TTS-duck API was retired with Epic 27 / ADR-076.
- **VoiceRouter** — `voice_router.rs` (350 LOC) — narrator + character archetype +
  creature type voice assignment for text framing. Genre pack integration. No
  longer drives TTS synthesis — voice metadata now flavors narration tone only.
- **ThemeRotator** — `theme_rotator.rs` (323 LOC) — anti-repetition track selection
  with per-mood play history.

### World State & Knowledge
- **LoreStore** — `lore.rs` (2,746 LOC) — knowledge indexing with category/keyword/
  semantic search. Budget-aware selection for Claude context. The largest module.
- **Conlang** — `conlang.rs` (902 LOC) — morpheme glossary for constructed languages.
  Template-based name generation with probabilistic patterns.
- **KnownFact** — `known_fact.rs` (47 LOC) — character knowledge accumulation.
  Monotonic, no decay.
- **FactionAgenda** — `faction_agenda.rs` (158 LOC) — faction goal tracking with
  urgency levels (Dormant -> Critical). Scene injection for narrator prompts.
- **SceneDirective** — `scene_directive.rs` (132 LOC) — scene instruction formatting.
  Composes trope beats + stakes + hints sorted by priority.
- **WorldMaterialization** — `world_materialization.rs` (94 LOC) — campaign maturity
  (Fresh/Early/Mid/Veteran) progressive world-building.

### Image Rendering Pipeline
- **SubjectExtractor** — `subject.rs` (387 LOC) — parse narration for render subjects.
  SubjectTier: Portrait / Scene / Landscape / Abstract.
- **RenderQueue** — `render_queue.rs` (470 LOC) — async image queue with SHA256
  content dedup. Max depth 1000.
- **BeatFilter** — `beat_filter.rs` (322 LOC) — render suppression by narrative weight,
  cooldown, burst rate.
- **PrerenderScheduler** — `prerender.rs` (417 LOC) — speculative rendering between
  narration turns, amortizing render latency against the player's reading/thinking
  gap. WasteTracker disables if hit rate drops below threshold.

### Game Mechanics
- **Inventory** — `inventory.rs` (346 LOC) — Item with narrative_weight-driven
  evolution (unnamed -> named at 0.5 -> evolved at 0.7). Carry limits, gold tracking.
  **Do NOT use Vec<String> for items. Use the Item struct.**
- **ChaseDepth** — `chase_depth.rs` (900 LOC) — camera modes, cinematography,
  terrain modifiers, danger levels, and outcome resolution for cinematic chases.
  Operates on a `StructuredEncounter` with `encounter_type == "chase"` — the old
  `ChaseState` was removed with story 16-2.
- **TropeEngine** — `trope.rs` (225 LOC) — trope runtime with passive progression +
  engagement multiplier. Escalation thresholds trigger FiredBeat events.
- **Disposition** — `disposition.rs` (223 LOC) — newtype i32 with Attitude derivation
  (Friendly > 10 / Neutral / Hostile < -10). ADR-020.
- **Progression** — `progression.rs` (46 LOC) — pure stat scaling functions with
  diminishing returns.
- **Ability** — `ability.rs` (56 LOC) — dual-voice representation (genre_description
  for players, mechanical_effect for engine). Involuntary flag for narrator triggers.
- **OCEAN** — `ocean.rs` — OceanProfile, OceanDimension, OceanShift, OceanShiftLog
  for Big Five personality tracking.

### Commands & Input
- **SlashRouter** — `slash_router.rs` (106 LOC) — command dispatch for /command input.
  CommandHandler trait, built-in /help. Extensible via trait.
- **Commands** — `commands.rs` (316 LOC) — /status, /inventory, /map, /save, /help.
  All complete and wired.

### Persistence
- **SqliteStore + PersistenceWorker** — `persistence.rs` (581 LOC) — actor-pattern
  persistence. One .db per genre/world session. Auto-save after every turn.
- **StateDelta** — `delta.rs` (239 LOC) — broadcast optimization. Computes changed
  fields between snapshots to avoid redundant client updates.

### Multiplayer Perception & Catch-Up
- **PerceptionRewriter** — `perception.rs` (200 LOC) — per-character narration variants
  based on status effects (blinded, charmed, dominated, etc.). RewriteStrategy trait
  enables test doubles. Wired into dispatch via `session_sync.rs` + `dispatch/mod.rs`.
  Production strategy: `ClaudeRewriteStrategy` in resonator agent (story 8-6).
- **OCEAN shift proposals** — `ocean_shift_proposals.rs` (155 LOC) — event-to-shift
  mapping (Betrayal, NearDeath, Victory, etc.) + `apply_ocean_shifts()` for batch
  application. Wired into `dispatch/npc_registry.rs`. Integration tests in
  `ocean_shift_wiring_story_15_2_tests.rs` (stories 10-6, 15-2).
- **Catch-up narration** — `catch_up.rs` (201 LOC) — generates arrival snapshot for
  mid-session joining players. GenerationStrategy trait with `ClaudeGenerationStrategy`
  impl in `dispatch/catch_up.rs`. Called from `dispatch/connect.rs` (story 8-8).

## NOT STARTED

- **Scenario system** — Epic 7 (BeliefState, gossip, clues, accusations). No code yet.

## Key Patterns

- **Composition over inheritance**: GameSnapshot composes domain structs; Character/Npc embed CreatureCore
- **Trait-based abstraction**: Combatant, CommandHandler, SessionStore, RewriteStrategy
- **Typed patches**: WorldStatePatch, NpcPatch for composable state mutations
- **Unified encounters**: StructuredEncounter (ADR-033) collapses combat/chase/standoff/negotiation into one model — no per-type state structs
- **Actor pattern**: PersistenceWorker owns SQLite Connection (single-threaded, !Send)
- **Newtype pattern**: Disposition(i32), TropeStatus, TurnStatus for semantic richness
