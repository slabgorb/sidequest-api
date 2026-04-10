//! Tests for story 15-15: Wire resolve_abilities into dispatch prompt.
//!
//! Verifies that resolved abilities are formatted for narrator prompt injection
//! and that the formatter follows the same pattern as format_lore_context/
//! format_chase_context.

use sidequest_game::{resolve_abilities, AffinityState};

// ============================================================================
// Game crate: format_abilities_context must exist and produce prompt text
// ============================================================================

#[test]
fn format_abilities_context_empty_list_returns_empty_string() {
    let result = sidequest_game::format_abilities_context(&[]);
    assert!(
        result.is_empty(),
        "Empty abilities list should produce empty string"
    );
}

#[test]
fn format_abilities_context_single_ability_included() {
    let abilities = vec!["Root-Bonding".to_string()];
    let result = sidequest_game::format_abilities_context(&abilities);
    assert!(
        result.contains("Root-Bonding"),
        "Formatted context should contain the ability name"
    );
    assert!(!result.is_empty());
}

#[test]
fn format_abilities_context_multiple_abilities_all_included() {
    let abilities = vec![
        "Root-Bonding".to_string(),
        "Fireball".to_string(),
        "Detect Corruption".to_string(),
    ];
    let result = sidequest_game::format_abilities_context(&abilities);
    for ability in &abilities {
        assert!(
            result.contains(ability.as_str()),
            "Missing ability: {ability}"
        );
    }
}

#[test]
fn format_abilities_context_contains_section_header() {
    let abilities = vec!["Root-Bonding".to_string()];
    let result = sidequest_game::format_abilities_context(&abilities);
    // Should have a header like "Character Abilities:" or similar, matching
    // the pattern of other format_*_context functions
    let lower = result.to_lowercase();
    assert!(
        lower.contains("abilit"),
        "Formatted context should contain a header mentioning abilities: got '{result}'"
    );
}

// ============================================================================
// Integration: resolve_abilities + format produces non-empty context
// ============================================================================

#[test]
fn resolve_then_format_produces_prompt_text() {
    let affinities = vec![AffinityState {
        name: "Nature".to_string(),
        tier: 1,
        progress: 5,
    }];
    let abilities = resolve_abilities(&affinities, &|name, tier| match (name, tier) {
        ("Nature", 0) => vec!["Herbalism".to_string()],
        ("Nature", 1) => vec!["Root-Bonding".to_string()],
        _ => vec![],
    });
    assert_eq!(abilities.len(), 2);

    let context = sidequest_game::format_abilities_context(&abilities);
    assert!(
        context.contains("Herbalism"),
        "Tier 0 ability should be in context"
    );
    assert!(
        context.contains("Root-Bonding"),
        "Tier 1 ability should be in context"
    );
}
