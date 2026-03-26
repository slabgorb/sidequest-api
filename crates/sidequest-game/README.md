# sidequest-game

Core game simulation — state, characters, combat, chase, inventory, and persistence.

This is the largest crate. It implements the game mechanics that agents manipulate
and the server exposes.

## Modules

| Module | Purpose |
|--------|---------|
| `state` | `GameSnapshot`, `WorldStatePatch`, `CombatPatch`, `ChasePatch` |
| `character` | Player `Character` model (narrative + mechanical) |
| `npc` | NPC definitions and behavior |
| `combat` | `CombatState`, `DamageEvent`, `StatusEffect`, round resolution |
| `combatant` | `Combatant` trait for anything that fights |
| `chase` | Beat-based cinematic chase sequences |
| `creature_core` | Shared attributes for characters and NPCs |
| `disposition` | NPC `Disposition` (numeric) → `Attitude` (qualitative) |
| `inventory` | `Inventory` and `Item` with narrative weight |
| `progression` | XP, leveling, damage/defense/HP scaling |
| `delta` | `StateDelta` and `StateSnapshot` for change tracking |
| `narrative` | `NarrativeEntry` — timestamped story log |
| `session` | `SessionManager` — save/load session lifecycle |
| `persistence` | `GameStore` — rusqlite-backed save system |
| `turn` | `TurnManager` and `TurnPhase` |
| `hp` | `clamp_hp` utility |

## Key design notes

- Characters carry both narrative identity and mechanical stats in one model
  ([ADR-007](../../../docs/adr/007-unified-character-model.md))
- State updates are JSON patches, not full replacements
  ([ADR-011](../../../docs/adr/011-world-state-json-patches.md))
- Disposition uses numeric values that derive qualitative attitudes
  ([ADR-020](../../../docs/adr/020-npc-disposition-system.md))
