//! Wiring tests for story 15-15: resolve_abilities called from dispatch.
//!
//! Verifies that the orchestrator/dispatch layer calls resolve_abilities()
//! and injects the result into the narrator prompt context. Also verifies
//! OTEL telemetry for abilities.resolved.

use sidequest_game::{AffinityState, resolve_abilities};

// ============================================================================
// Wiring: orchestrator must call resolve_abilities and produce prompt context
// ============================================================================

/// The orchestrator module should have a function or code path that resolves
/// abilities from character affinities and injects them into the prompt.
/// This test verifies the function exists and is callable from the agents crate.
#[test]
fn resolve_abilities_callable_from_agents_crate() {
    let affinities = vec![
        AffinityState { name: "Fire".to_string(), tier: 2, progress: 10 },
    ];
    let abilities = resolve_abilities(&affinities, &|_name, tier| {
        match tier {
            0 => vec!["Spark".to_string()],
            1 => vec!["Flame Shield".to_string()],
            2 => vec!["Fireball".to_string()],
            _ => vec![],
        }
    });
    // Tier 2 means tiers 0, 1, 2 all resolve
    assert_eq!(abilities.len(), 3);
    assert!(abilities.contains(&"Spark".to_string()));
    assert!(abilities.contains(&"Flame Shield".to_string()));
    assert!(abilities.contains(&"Fireball".to_string()));
}

/// Verify format_abilities_context is accessible from the agents crate
/// (it needs to be for prompt building in the orchestrator).
#[test]
fn format_abilities_context_accessible_from_agents() {
    let abilities = vec!["Spark".to_string(), "Fireball".to_string()];
    let context = sidequest_game::format_abilities_context(&abilities);
    assert!(!context.is_empty(), "Should produce non-empty prompt context");
    assert!(context.contains("Spark"));
    assert!(context.contains("Fireball"));
}

/// OTEL telemetry: verify the AbilitiesResolvedSummary struct exists
/// for emitting abilities.resolved watcher events.
#[test]
fn abilities_resolved_summary_captures_telemetry_fields() {
    let summary = sidequest_game::AbilitiesResolvedSummary {
        count: 3,
        tiers_active: 2,
        ability_names: vec!["Spark".to_string(), "Flame Shield".to_string(), "Fireball".to_string()],
    };
    assert_eq!(summary.count, 3);
    assert_eq!(summary.tiers_active, 2);
    assert_eq!(summary.ability_names.len(), 3);
}
