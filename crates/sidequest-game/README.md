# sidequest-game

Core game simulation — ~23,700 LOC across 59 modules covering state, characters,
combat, chase, inventory, audio, rendering, persistence, and multiplayer.

This is the largest crate. It implements the game mechanics that agents manipulate
and the server exposes.

## Modules

### Core Game State
| Module | Purpose |
|--------|---------|
| `state` | `GameSnapshot`, `WorldStatePatch`, `CombatPatch`, `ChasePatch` |
| `character` | Player `Character` model (narrative + mechanical) |
| `npc` | NPC with disposition-based attitude, identity-locked fields |
| `creature_core` | Shared combat abstraction for Character/NPC (composition) |
| `builder` | Multi-phase character creation state machine |
| `delta` | `StateDelta` / `StateSnapshot` for change tracking |

### Combat & Turns
| Module | Purpose |
|--------|---------|
| `combat` | `CombatState`, `DamageEvent`, `StatusEffect`, round resolution |
| `combatant` | `Combatant` trait — shared combat interface |
| `chase` | Beat-based cinematic chase sequences (Footrace/Stealth/Negotiation) |
| `chase_depth` | Camera modes, cinematography, terrain modifiers, danger, outcomes |
| `turn` | `TurnManager`, `TurnPhase` (5-phase pipeline) |
| `turn_mode` | FreePlay / Structured / Cinematic state machine |
| `barrier` | `TurnBarrier` — concurrent turn coordination with adaptive timeout |
| `turn_reminder` | Idle player detection |
| `hp` | `clamp_hp` utility (fixes Python overflow bug) |

### Multiplayer
| Module | Purpose |
|--------|---------|
| `multiplayer` | `MultiplayerSession` — player→Character map, force_resolve_turn() |
| `guest_npc` | Guest players controlling NPCs with limited agency |

### Narrative & Audio
| Module | Purpose |
|--------|---------|
| `narrative` | `NarrativeEntry` — timestamped story log |
| `tension_tracker` | Dual-track pacing (action + stakes), `PacingHint` injection |
| `music_director` | Mood classification, track selection from genre pack |
| `audio_mixer` | 3-channel cue-driven mixer (Music, SFX, Ambience) |
| `voice_router` | Narrator + archetype + creature type voice assignment (text framing) |
| `theme_rotator` | Anti-repetition track selection with per-mood history |

### World State & Knowledge
| Module | Purpose |
|--------|---------|
| `lore` | `LoreStore` — category/keyword/semantic search, budget-aware selection |
| `conlang` | Morpheme glossary, template-based constructed language name generation |
| `known_fact` | Character knowledge accumulation (monotonic, no decay) |
| `faction_agenda` | Faction goals with urgency levels (Dormant → Critical) |
| `scene_directive` | Scene instruction formatting for narrator prompts |
| `world_materialization` | Campaign maturity (Fresh/Early/Mid/Veteran) |
| `trope` | `TropeEngine` — passive progression + engagement multiplier |

### Image Rendering Pipeline
| Module | Purpose |
|--------|---------|
| `subject` | `SubjectExtractor` — parse narration for render subjects (4 tiers) |
| `render_queue` | Async image queue with SHA256 content dedup |
| `beat_filter` | Render suppression by narrative weight, cooldown, burst rate |
| `prerender` | Speculative rendering between narration turns, `WasteTracker` |

### Game Mechanics
| Module | Purpose |
|--------|---------|
| `inventory` | `Inventory` and `Item` with narrative weight evolution |
| `disposition` | `Disposition(i32)` newtype → `Attitude` derivation |
| `progression` | XP, leveling, damage/defense/HP scaling (diminishing returns) |
| `ability` | Dual-voice (genre_description + mechanical_effect) |
| `ocean` | `OceanProfile`, `OceanDimension`, `OceanShift`, `OceanShiftLog` |

### Commands & Input
| Module | Purpose |
|--------|---------|
| `slash_router` | `/command` dispatch, `CommandHandler` trait |
| `commands` | `/status`, `/inventory`, `/map`, `/save`, `/help` |

### Persistence
| Module | Purpose |
|--------|---------|
| `persistence` | Actor-pattern `PersistenceWorker` + `SqliteStore` |

## Key design notes

- Characters carry both narrative identity and mechanical stats in one model
  ([ADR-007](../../../docs/adr/007-unified-character-model.md))
- State updates are JSON patches, not full replacements
  ([ADR-011](../../../docs/adr/011-world-state-json-patches.md))
- Disposition uses numeric values that derive qualitative attitudes
  ([ADR-020](../../../docs/adr/020-npc-disposition-system.md))
- Dual-track tension model for narrative pacing
  ([ADR-024](../../../docs/adr/024-dual-track-tension-model.md))
- Cinematic chase engine with camera modes
  ([ADR-017](../../../docs/adr/017-cinematic-chase-engine.md))
