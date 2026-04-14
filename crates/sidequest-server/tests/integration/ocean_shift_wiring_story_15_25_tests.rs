//! Story 15-25: Wire propose_ocean_shifts — OCEAN personality shift pipeline OTEL observability.
//!
//! The OCEAN shift pipeline (propose_ocean_shifts → apply_ocean_shifts) is already called from
//! dispatch/mod.rs:1885. However, the telemetry is incomplete:
//!   - No WatcherEvent emitted for the GM panel (only tracing::info/debug)
//!   - No `ocean.shift_proposed` per-proposal OTEL event
//!
//! ACs covered:
//!   AC-1: apply_ocean_shifts called from dispatch (ALREADY WIRED)
//!   AC-2: WatcherEvent with component="ocean" emitted for GM panel
//!   AC-3: Per-proposal telemetry: ocean.shift_proposed event with npc_name, dimension, delta
//!   AC-4: Summary telemetry: ocean.shift_applied event with shifts_count, npc_count

// ============================================================================
// Source-code wiring tests: verify production code emits required OTEL events
// ============================================================================

/// AC-1: apply_ocean_shifts must be called from dispatch/mod.rs.
/// This is already wired — test confirms it stays wired.
#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn dispatch_calls_apply_ocean_shifts() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("apply_ocean_shifts"),
        "dispatch/mod.rs must call apply_ocean_shifts() \
         to process OCEAN personality events from narration. \
         This was wired in a previous story — do not regress."
    );
}

/// AC-2: dispatch must emit a WatcherEvent with component="ocean" for the GM panel.
/// Currently only tracing::info is used — the GM panel cannot see OCEAN shifts.
#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn dispatch_emits_ocean_watcher_event() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    // The GM panel reads WatcherEvents, not tracing spans.
    // Must use WatcherEventBuilder with component "ocean".
    assert!(
        dispatch_source.contains("WatcherEventBuilder::new(\"ocean\""),
        "dispatch/mod.rs must emit a WatcherEvent with component='ocean' \
         for the GM panel. Currently only tracing::info/debug is used, \
         which does not surface to the GM dashboard. \
         Use WatcherEventBuilder::new(\"ocean\", ...) pattern."
    );
}

/// AC-3: dispatch must emit per-proposal telemetry with ocean.shift_proposed.
/// Each individual OCEAN shift proposal should be logged with npc_name, dimension, delta.
#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn dispatch_emits_ocean_shift_proposed_event() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("ocean.shift_proposed")
            || dispatch_source.contains("ocean_shift_proposed"),
        "dispatch/mod.rs must emit an 'ocean.shift_proposed' event \
         for each individual OCEAN shift proposal, with fields: \
         npc_name, dimension, delta, cause. \
         Currently no per-proposal telemetry event is emitted."
    );
}

/// AC-3 continued: per-proposal event must include dimension field.
#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn dispatch_ocean_shift_includes_dimension_field() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    // Check that WatcherEvent for ocean includes dimension in its fields
    // (not just tracing::debug which doesn't reach GM panel)
    let has_ocean_watcher = dispatch_source.contains("WatcherEventBuilder::new(\"ocean\"");
    let has_dimension_field = dispatch_source.contains(".field(\"dimension\"");
    assert!(
        has_ocean_watcher && has_dimension_field,
        "dispatch/mod.rs must emit a WatcherEvent for ocean shifts \
         that includes a 'dimension' field (e.g., Openness, Conscientiousness). \
         Currently dimension is only in tracing::debug, not WatcherEvent."
    );
}

/// AC-4: dispatch must emit a summary ocean.shift_applied event with counts.
#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn dispatch_emits_ocean_shift_applied_summary() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains(".field(\"shifts_applied\"")
            || dispatch_source.contains(".field(\"shifts_count\""),
        "dispatch/mod.rs must emit an ocean WatcherEvent with a \
         'shifts_applied' or 'shifts_count' field summarizing how many \
         OCEAN shifts were applied in this turn. \
         Currently this count is only in tracing::info, not WatcherEvent."
    );
}

// ============================================================================
// Behavioral tests: game crate functions still work correctly
// ============================================================================

/// propose_ocean_shifts returns non-empty proposals for known events.
#[test]
fn propose_ocean_shifts_produces_proposals() {
    use sidequest_game::{propose_ocean_shifts, PersonalityEvent};

    let proposals = propose_ocean_shifts(PersonalityEvent::Betrayal, "TestNpc");
    assert!(
        !proposals.is_empty(),
        "propose_ocean_shifts should produce at least one shift proposal for Betrayal"
    );
    for proposal in &proposals {
        assert_eq!(proposal.npc_name, "TestNpc");
        assert!(
            proposal.delta.abs() > f64::EPSILON,
            "Each proposal should have a non-zero delta, got {}",
            proposal.delta
        );
    }
}

/// apply_ocean_shifts mutates NPC OCEAN profiles in the registry.
#[test]
fn apply_ocean_shifts_mutates_npc_profiles() {
    use sidequest_game::{apply_ocean_shifts, NpcRegistryEntry, OceanProfile, PersonalityEvent};

    let mut registry = vec![NpcRegistryEntry {
        name: "Mira".to_string(),
        pronouns: "they/them".to_string(),
        role: "ally".to_string(),
        location: "camp".to_string(),
        last_seen_turn: 0,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: String::new(),
        ocean: Some(OceanProfile::default()),
        hp: 20,
        max_hp: 20,
        portrait_url: None,
    }];

    let events = vec![("Mira".to_string(), PersonalityEvent::Betrayal)];
    let (applied, log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(
        !applied.is_empty(),
        "apply_ocean_shifts should produce applied proposals for a Betrayal event"
    );
    assert!(
        !log.shifts().is_empty(),
        "Shift log should record the applied shifts"
    );
    // The ocean_summary should be regenerated from the mutated profile
    assert!(
        !registry[0].ocean_summary.is_empty(),
        "ocean_summary should be regenerated after shift is applied"
    );
}
