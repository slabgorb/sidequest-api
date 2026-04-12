//! Playtest 2026-04-11 regression guard: TurnComplete telemetry must
//! include the fields the OTEL dashboard reads.
//!
//! Two adjacent bugs were reported during playtest 2026-04-11:
//!
//! Bug 1 — Tier "?":
//!   The Turn Details panel showed `Tier: ?` for every turn. The
//!   `extraction_tier` value (full / delta per ADR-066) was being emitted
//!   on the `AgentSpanClose` event in dispatch/mod.rs:1127, but the
//!   dashboard's `TimelineTab` reads its data from the `TurnComplete`
//!   event in `telemetry.rs::record_turn_telemetry`. The field simply
//!   didn't exist on the event the UI was reading.
//!
//! Bug 2 — Turn # collides across sessions:
//!   Two different sessions in the same genre/world both showed
//!   `#1 narrator` rows in the dashboard Timeline, mingled together with
//!   no way to tell them apart. `turn_id` resets per session, but the
//!   TurnComplete event didn't carry enough metadata to group rows by
//!   session. (player_id alone is insufficient — same player can play
//!   the same world twice.)
//!
//! Fix (telemetry.rs::record_turn_telemetry):
//!   Add three fields to the TurnComplete event:
//!     - `extraction_tier` — sourced from `result.prompt_tier` so the
//!       dashboard can display it
//!     - `genre` — sourced from `ctx.genre_slug` so the client can group
//!     - `world` — sourced from `ctx.world_slug` so the client can group
//!
//! Together (`player_id`, `genre`, `world`) form a stable session
//! identifier the dashboard can use to draw session dividers.
//!
//! These tests are source-inspection (matching the convention of
//! `npc_turns_beat_system_story_28_8_tests.rs`) because the actual
//! telemetry pipeline involves a global subscriber and is not easily
//! captured in a unit test.

#[test]
fn turn_complete_emits_extraction_tier_field() {
    let src = include_str!("../../src/dispatch/telemetry.rs");

    // The fix must add `extraction_tier` to the TurnComplete builder, sourced
    // from result.prompt_tier (the orchestrator's per-turn tier selection).
    assert!(
        src.contains(r#".field("extraction_tier", &result.prompt_tier)"#),
        "telemetry.rs::record_turn_telemetry must add `.field(\"extraction_tier\", &result.prompt_tier)` \
         to the TurnComplete WatcherEventBuilder. Without this field on the \
         TurnComplete event, the dashboard's TimelineTab Turn Details panel \
         will show `Tier: ?` because it reads extraction_tier from the \
         TurnComplete fields, not the AgentSpanClose fields. Playtest \
         2026-04-11 regression."
    );
}

#[test]
fn turn_complete_emits_genre_and_world_fields() {
    let src = include_str!("../../src/dispatch/telemetry.rs");

    // Both genre and world must be on the TurnComplete event so the dashboard
    // can group turns by session boundary. (player_id, genre, world) is the
    // stable session identifier.
    assert!(
        src.contains(r#".field("genre", ctx.genre_slug)"#),
        "telemetry.rs::record_turn_telemetry must add genre to the TurnComplete event. \
         Without this, the dashboard cannot draw session dividers when the \
         same player has Turn #1 from two different worlds in the timeline."
    );
    assert!(
        src.contains(r#".field("world", ctx.world_slug)"#),
        "telemetry.rs::record_turn_telemetry must add world to the TurnComplete event. \
         Without this, the dashboard cannot draw session dividers when the \
         same player has Turn #1 from two different sessions in the same world."
    );
}

#[test]
fn turn_complete_still_carries_existing_dashboard_fields() {
    // Regression guard for the existing fields the dashboard depends on.
    // If a future refactor accidentally drops one of these, the dashboard
    // would silently lose data and we'd be debugging "Tokens: 0 in / 0 out"
    // or "Intent: ?" instead of getting a build error.
    let src = include_str!("../../src/dispatch/telemetry.rs");

    let required_fields = [
        ("turn_id", r#".field("turn_id", turn_number)"#),
        ("turn_number", r#".field("turn_number", turn_number)"#),
        ("player_input", r#".field("player_input", ctx.action)"#),
        (
            "classified_intent",
            r#".field_opt("classified_intent", &result.classified_intent)"#,
        ),
        (
            "agent_name",
            r#".field_opt("agent_name", &result.agent_name)"#,
        ),
        (
            "agent_duration_ms",
            r#".field("agent_duration_ms", agent_ms)"#,
        ),
        (
            "is_degraded",
            r#".field("is_degraded", result.is_degraded)"#,
        ),
        ("player_id", r#".field("player_id", ctx.player_id)"#),
        (
            "token_count_in",
            r#".field_opt("token_count_in", &result.token_count_in)"#,
        ),
        (
            "token_count_out",
            r#".field_opt("token_count_out", &result.token_count_out)"#,
        ),
        ("spans", r#".field("spans", &spans)"#),
        (
            "total_duration_ms",
            r#".field("total_duration_ms", total_ms)"#,
        ),
    ];

    for (name, pattern) in &required_fields {
        assert!(
            src.contains(pattern),
            "telemetry.rs must still contain the {} field on the TurnComplete \
             event. Pattern not found: `{}`. The dashboard depends on this \
             field — silently dropping it would break the Turn Details panel.",
            name,
            pattern
        );
    }
}
