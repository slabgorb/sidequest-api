---
story_id: "2-2"
jira_key: ""
epic: "2"
workflow: "tdd"
---

# Story 2-2: Session Actor — Per-Connection Tokio Task, Connect/Create/Play State Machine, Genre Binding

## Story Details
- **ID:** 2-2
- **Epic:** 2 (Core Game Loop Integration)
- **Workflow:** tdd
- **Points:** 5
- **Priority:** p0
- **Stack Parent:** 2-1 (server bootstrap, WebSocket handler)

## Workflow Tracking
**Workflow:** tdd
**Phase:** setup
**Phase Started:** 2026-03-26T00:50:00Z

### Phase History
| Phase | Started | Ended | Duration |
|-------|---------|-------|----------|
| setup | 2026-03-26T00:50:00Z | - | - |

## Story Context

**Key Points:**
- Story 2-1 built the WebSocket handler with PlayerId and connection tracking
- This story adds the Session state machine: Connect → Create → Play
- Each WS connection owns a Session tokio task with its own genre pack binding
- The Session dispatches messages based on current state

**What's In Scope:**
- Session enum: Connecting, Creating, Playing states
- Per-connection tokio task that owns the session state
- Genre pack binding on SESSION_EVENT{connect}
- State-appropriate message dispatch (reject out-of-phase messages)
- Session cleanup on disconnect
- Connected response: Server sends SESSION_EVENT{connected} with has_character flag
- Multiple sessions: Two clients can have independent session states

**Acceptance Criteria:**
1. Session state machine: Connect → Create → Play transitions work correctly
2. Genre binding: SESSION_EVENT{connect} with genre+world loads genre pack reference
3. State dispatch: Messages are routed based on current session state
4. Out-of-phase rejection: PLAYER_ACTION in Connecting state returns ERROR
5. Session cleanup: Disconnecting client properly cleans up session resources
6. Connected response: Server sends SESSION_EVENT{connected} with has_character flag
7. Multiple sessions: Two clients can have independent session states

## Sm Assessment

Story 2-2 is ready for TDD red phase. Session created, branch feat/2-2-session-actor exists in sidequest-api. Depends on 2-1 which is complete and merged. Handing off to TEA for failing tests.

## Delivery Findings

No upstream findings.

<!-- Agents: append findings below this line. Do not edit other agents' entries. -->

## Design Deviations

None yet.

<!-- Agents: append deviations below this line. Do not edit other agents' entries. -->
