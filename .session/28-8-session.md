---
story_id: "28-8"
epic: "28"
workflow: "tdd"
branch: "feat/28-8-npc-turns-beat-system"
---
# Story 28-8: NPC turns through beat system — NPCs mechanically act every round

## Story Details
- **ID:** 28-8
- **Epic:** 28 — Unified Encounter Engine
- **Jira Key:** none (personal project)
- **Workflow:** tdd
- **Stack Parent:** 28-7 (StructuredEncounter promotion complete)

## Objective

In the unified encounter engine, NPCs must act mechanically every round. Currently, the narrator says "the goblin attacks" but `resolve_attack()` is never called for the NPC. This story wires NPC beat selection and mechanical resolution into the dispatch loop.

## Acceptance Criteria

1. Each NPC actor in an encounter gets a beat selection per round
2. For combat encounters, NPC beats default to "attack" targeting a player
3. For other encounters, creature_smith selects NPC beats based on disposition and role
4. Dispatch loops through all actors, calls `apply_beat()` for each, resolves the stat_check
5. Every NPC action produces OTEL events
6. GM panel shows exactly what each NPC did mechanically

## Workflow Tracking
**Workflow:** tdd
**Phase:** setup
**Phase Started:** 2026-04-08T17:30Z

### Phase History
| Phase | Started | Ended | Duration |
|-------|---------|-------|----------|
| setup | 2026-04-08T17:30Z | - | - |

## Delivery Findings

Agents record upstream observations discovered during their phase.
Each finding is one list item. Use "No upstream findings" if none.

**Types:** Gap, Conflict, Question, Improvement
**Urgency:** blocking, non-blocking

<!-- Agents: append findings below this line. Do not edit other agents' entries. -->

## Design Deviations

Agents log spec deviations as they happen — not after the fact.
Each entry: what was changed, what the spec said, and why.

<!-- Agents: append deviations below this line. Do not edit other agents' entries. -->
