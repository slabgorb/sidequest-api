---
story_id: "15-10"
jira_key: ""
epic: "15"
workflow: "tdd"
---

# Story 15-10: Wire seed_lore_from_char_creation

## Story Details

- **ID:** 15-10
- **Epic:** 15 (Playtest Debt Cleanup)
- **Jira Key:** N/A (personal project)
- **Workflow:** tdd
- **Stack Parent:** none
- **Points:** 2
- **Priority:** p0

## Story Context

Character creation scenes chosen by the player contain rich backstory, hooks, and world context that should prime the lore store before the first turn. The function `seed_lore_from_char_creation()` is fully implemented in `sidequest-game/src/lore.rs` but has **zero callers in production code**.

**Current State:**
- `seed_lore_from_genre_pack()` IS called at session start (lib.rs:2182, 2388)
- `seed_lore_from_char_creation()` is never called anywhere in sidequest-server

**Required Fix:**
Call `seed_lore_from_char_creation()` at the end of `dispatch_character_creation()` after the character is finalized (around line 2615-2640 where character data is synced).

**OTEL Event Required:**
- `lore.char_creation_seeded` — fragment_count, categories

## Workflow Tracking

**Workflow:** tdd
**Phase:** setup
**Phase Started:** 2026-04-01T00:00:00Z

### Phase History

| Phase | Started | Ended | Duration |
|-------|---------|-------|----------|
| setup | 2026-04-01 | - | - |

## Delivery Findings

No upstream findings.

<!-- Agents: append findings below this line. Do not edit other agents' entries. -->

## Design Deviations

No design deviations.

<!-- Agents: append deviations below this line. Do not edit other agents' entries. -->

## Implementation Plan

1. **Identify the caller context**: The `dispatch_character_creation()` function in `sidequest-server/src/lib.rs` (line 2475+)
2. **Extract scenes from CharacterBuilder**: Use the builder's scenes field or reconstruct from scene_results
3. **Call seed_lore_from_char_creation()**: Pass lore_store and the selected scenes
4. **Count fragments added**: Use the return value for OTEL event
5. **Wire OTEL event**: Send `lore.char_creation_seeded` with fragment_count and categories
6. **Test coverage**: Write integration test that verifies the call is made and lore fragments are seeded

## Key Code Locations

- **Function to wire**: `sidequest_game::seed_lore_from_char_creation` (lore.rs:315)
- **Caller location**: `sidequest-server/src/lib.rs:dispatch_character_creation()` (~line 2615-2640)
- **CharacterBuilder**: Has `scenes` field and `scene_results()` method (builder.rs)
- **OTEL event patterns**: See existing events in dispatch_character_creation (e.g., character_built event around line 2585)
