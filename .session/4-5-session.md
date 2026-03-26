---
story_id: "4-5"
epic: "4"
epic_title: "Media Integration"
workflow: "tdd"
---
# Story 4-5: IMAGE message broadcast — deliver rendered images to connected clients via WebSocket

## Story Details
- **ID:** 4-5
- **Title:** IMAGE message broadcast — deliver rendered images to connected clients via WebSocket
- **Points:** 3
- **Priority:** p1
- **Epic:** 4 — Media Integration
- **Workflow:** tdd
- **Stack Parent:** 4-4 (render-queue)

## Story Description

When the render queue (4-4) completes an image render, broadcast an IMAGE message to all connected WebSocket clients. This wires the render pipeline output to the frontend. Uses the GameMessage protocol from sidequest-protocol crate.

## Workflow Tracking

**Workflow:** tdd
**Phase:** setup
**Phase Started:** 2026-03-26T00:00:00Z

### Phase History
| Phase | Started | Ended | Duration |
|-------|---------|-------|----------|
| setup | 2026-03-26T00:00:00Z | - | - |

## Implementation Context

### Architecture Notes
- **Dependency:** 4-4 (render queue) must be complete — this story consumes the rendered image output
- **Protocol:** Uses `GameMessage::Image` from sidequest-protocol crate
- **Target:** sidequest-server crate — integrate with existing WebSocket broadcast infrastructure
- **Flow:** RenderQueue completion → IMAGE message → Session actor broadcast → Connected clients

### Key Files (Expected)
- `crates/sidequest-server/src/session.rs` — Session actor, broadcast infrastructure
- `crates/sidequest-protocol/src/lib.rs` — GameMessage::Image variant
- `crates/sidequest-server/src/render_integration.rs` (new) — Render queue listener and IMAGE broadcaster

### Design Considerations
1. **Threading:** RenderQueue runs in daemon subprocess; IMAGE broadcast must be async-safe from Tokio actor
2. **Message Format:** IMAGE payload should include render hash, image data (or URL reference), and metadata
3. **Client Delivery:** All connected WebSocket clients receive IMAGE (consider filtering later if needed)
4. **Backpressure:** If a client disconnects mid-broadcast, handle gracefully

## Delivery Findings

No upstream findings.

<!-- Agents: append findings below this line. Do not edit other agents' entries. -->

## Design Deviations

None recorded yet.

<!-- Agents: append deviations below this line. Do not edit other agents' entries. -->
