//! Tests for trope inheritance resolution (resolve.rs) and ScenarioPack model correctness.
//!
//! Story 1-5: Trope inheritance + scenario packs — extends resolution, cycle detection, ScenarioPack.
//!
//! These tests target edge cases and rule-enforcement beyond the basic happy-path
//! coverage in integration_tests.rs. Focus areas:
//! - Multi-level inheritance chains
//! - Self-cycles and depth limits (CWE-674)
//! - Merge field semantics (empty child inherits, non-empty child overrides)
//! - Slugification in extends matching
//! - Abstract trope filtering
//! - PassiveProgression and escalation inheritance
//! - NonBlankString validation through deserialization (rule #8)

use sidequest_genre::{GenreError, ScenarioPack, TropeDefinition};

// ═══════════════════════════════════════════════════════════
// Helper: build TropeDefinition from YAML for concise tests
// ═══════════════════════════════════════════════════════════

fn tropes_from_yaml(yaml: &str) -> Vec<TropeDefinition> {
    serde_yaml::from_str(yaml).expect("test YAML should parse")
}

// ═══════════════════════════════════════════════════════════
// Multi-level inheritance (A extends B extends C)
// ═══════════════════════════════════════════════════════════

#[test]
fn multi_level_inheritance_resolves_grandparent_fields() {
    // Genre-level grandparent
    let genre = tropes_from_yaml(
        r#"
- name: Archetype Root
  abstract: true
  category: recurring
  triggers:
    - ancient trigger
  narrative_hints:
    - root guidance
  tension_level: 0.3
  resolution_patterns:
    - the cycle repeats
"#,
    );

    // World tropes: B extends Root, C extends B
    let world = tropes_from_yaml(
        r#"
- name: Mid Trope
  extends: archetype-root
  triggers:
    - mid trigger
- name: Leaf Trope
  extends: mid-trope
  description: The final inheritor
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world)
        .expect("multi-level inheritance should resolve");

    assert_eq!(resolved.len(), 2, "both world tropes should appear");

    // Mid Trope inherits category from Root, overrides triggers
    let mid = resolved
        .iter()
        .find(|t| t.name.as_str() == "Mid Trope")
        .unwrap();
    assert_eq!(
        mid.category, "recurring",
        "mid should inherit category from root"
    );
    assert!(
        mid.triggers.iter().any(|t| t.contains("mid trigger")),
        "mid should have its own triggers"
    );
    assert!(
        !mid.triggers.iter().any(|t| t.contains("ancient")),
        "mid should NOT have root triggers (overridden)"
    );

    // Leaf Trope extends Mid Trope — inherits mid's triggers (which overrode root's)
    let leaf = resolved
        .iter()
        .find(|t| t.name.as_str() == "Leaf Trope")
        .unwrap();
    assert_eq!(
        leaf.description.as_deref(),
        Some("The final inheritor"),
        "leaf keeps its own description"
    );
    // Leaf has no triggers of its own → inherits from Mid Trope
    assert!(
        leaf.triggers.iter().any(|t| t.contains("mid trigger")),
        "leaf should inherit triggers from mid parent"
    );
}

// ═══════════════════════════════════════════════════════════
// Self-cycle (A extends A)
// ═══════════════════════════════════════════════════════════

#[test]
fn self_cycle_detected() {
    let world = tropes_from_yaml(
        r#"
- name: Ouroboros
  extends: ouroboros
  category: conflict
"#,
    );

    let result = sidequest_genre::resolve_trope_inheritance(&[], &world);
    assert!(
        result.is_err(),
        "self-referencing extends should be detected as a cycle"
    );
    match result.unwrap_err() {
        GenreError::CycleDetected { .. } => {}
        other => panic!("expected CycleDetected, got: {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════
// Three-node cycle (A → B → C → A)
// ═══════════════════════════════════════════════════════════

#[test]
fn three_node_cycle_detected() {
    let world = tropes_from_yaml(
        r#"
- name: Alpha
  extends: charlie
  category: conflict
- name: Bravo
  extends: alpha
  category: conflict
- name: Charlie
  extends: bravo
  category: conflict
"#,
    );

    let result = sidequest_genre::resolve_trope_inheritance(&[], &world);
    assert!(result.is_err(), "3-node cycle should be detected");
    match result.unwrap_err() {
        GenreError::CycleDetected { .. } => {}
        other => panic!("expected CycleDetected, got: {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════
// Depth limit (CWE-674: unbounded recursion)
// ═══════════════════════════════════════════════════════════

#[test]
fn depth_limit_rejects_excessively_deep_chain() {
    // Build a chain of 70 tropes: trope-0 extends trope-1 extends ... extends trope-69
    // MAX_INHERITANCE_DEPTH is 64, so this should fail
    let mut yaml = String::new();
    for i in 0..70 {
        if i < 69 {
            yaml.push_str(&format!(
                "- name: Trope {i}\n  extends: trope-{}\n  category: conflict\n",
                i + 1
            ));
        } else {
            // Last trope has no extends (chain root)
            yaml.push_str(&format!("- name: Trope {i}\n  category: conflict\n"));
        }
    }

    let world = tropes_from_yaml(&yaml);
    let result = sidequest_genre::resolve_trope_inheritance(&[], &world);
    assert!(
        result.is_err(),
        "chain exceeding MAX_INHERITANCE_DEPTH should be rejected"
    );
}

// ═══════════════════════════════════════════════════════════
// Merge field semantics — child overrides vs inherits
// ═══════════════════════════════════════════════════════════

#[test]
fn merge_child_description_overrides_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: revelation
  description: Parent description
  tension_level: 0.4
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
  description: Child description
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    let derived = &resolved[0];
    assert_eq!(
        derived.description.as_deref(),
        Some("Child description"),
        "child description should override parent"
    );
}

#[test]
fn merge_child_inherits_description_when_absent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: revelation
  description: Inherited description
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    let derived = &resolved[0];
    assert_eq!(
        derived.description.as_deref(),
        Some("Inherited description"),
        "child should inherit parent description when absent"
    );
}

#[test]
fn merge_child_inherits_tension_level_when_absent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  tension_level: 0.7
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].tension_level,
        Some(0.7),
        "child should inherit tension_level from parent"
    );
}

#[test]
fn merge_child_overrides_tension_level() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  tension_level: 0.3
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
  tension_level: 0.9
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].tension_level,
        Some(0.9),
        "child tension_level should override parent"
    );
}

#[test]
fn merge_empty_child_triggers_inherits_from_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  triggers:
    - parent trigger one
    - parent trigger two
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].triggers.len(),
        2,
        "child with no triggers should inherit parent's triggers"
    );
    assert_eq!(resolved[0].triggers[0], "parent trigger one");
}

#[test]
fn merge_non_empty_child_triggers_overrides_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  triggers:
    - parent trigger
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
  triggers:
    - child trigger
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(resolved[0].triggers.len(), 1);
    assert_eq!(
        resolved[0].triggers[0], "child trigger",
        "child triggers should fully replace parent triggers"
    );
}

#[test]
fn merge_empty_child_tags_inherits_from_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  tags: [dark, foreboding]
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(resolved[0].tags, vec!["dark", "foreboding"]);
}

#[test]
fn merge_resolution_hints_inherited_when_child_absent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: conflict
  resolution_hints:
    - hero prevails
    - sacrifice required
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    let hints = resolved[0]
        .resolution_hints
        .as_ref()
        .expect("should inherit resolution_hints");
    assert_eq!(hints.len(), 2);
}

#[test]
fn merge_resolution_patterns_inherited_when_child_absent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: recurring
  resolution_patterns:
    - the mentor reveals the truth
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    let patterns = resolved[0]
        .resolution_patterns
        .as_ref()
        .expect("should inherit resolution_patterns");
    assert_eq!(patterns[0], "the mentor reveals the truth");
}

// ═══════════════════════════════════════════════════════════
// Escalation and PassiveProgression inheritance
// ═══════════════════════════════════════════════════════════

#[test]
fn merge_inherits_escalation_from_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: climax
  escalation:
    - at: 0.25
      event: warning signs
      stakes: low
    - at: 0.75
      event: climax approaches
      stakes: high
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].escalation.len(),
        2,
        "child should inherit parent escalation beats"
    );
    assert!((resolved[0].escalation[0].at - 0.25).abs() < f64::EPSILON);
    assert_eq!(resolved[0].escalation[1].event, "climax approaches");
}

#[test]
fn merge_child_escalation_overrides_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: climax
  escalation:
    - at: 0.5
      event: parent beat
      stakes: medium
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
  escalation:
    - at: 0.1
      event: child beat
      stakes: low
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(resolved[0].escalation.len(), 1);
    assert_eq!(
        resolved[0].escalation[0].event, "child beat",
        "child escalation should fully replace parent"
    );
}

#[test]
fn merge_inherits_passive_progression_from_parent() {
    let genre = tropes_from_yaml(
        r#"
- name: Base
  abstract: true
  category: recurring
  passive_progression:
    rate_per_turn: 0.02
    rate_per_day: 0.05
    accelerators: [combat, dialogue]
    decelerators: [rest]
    accelerator_bonus: 0.01
    decelerator_penalty: 0.005
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Derived
  extends: base
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    let prog = resolved[0]
        .passive_progression
        .as_ref()
        .expect("should inherit passive_progression");
    assert!((prog.rate_per_turn - 0.02).abs() < f64::EPSILON);
    assert!((prog.rate_per_day - 0.05).abs() < f64::EPSILON);
    assert_eq!(prog.accelerators, vec!["combat", "dialogue"]);
}

// ═══════════════════════════════════════════════════════════
// Slugification matching
// ═══════════════════════════════════════════════════════════

#[test]
fn extends_matches_by_slug_with_spaces_and_case() {
    let genre = tropes_from_yaml(
        r#"
- name: The Dark Mentor
  abstract: true
  category: recurring
  triggers:
    - mentor appears
"#,
    );
    // extends uses the slug form (lowercase, hyphens)
    let world = tropes_from_yaml(
        r#"
- name: Local Variant
  extends: the-dark-mentor
  description: A local version
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world)
        .expect("slug matching should resolve spaces and case");
    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].category, "recurring",
        "should inherit from slug-matched parent"
    );
}

#[test]
fn extends_matches_case_insensitively() {
    let genre = tropes_from_yaml(
        r#"
- name: UPPER CASE TROPE
  abstract: true
  category: conflict
  tension_level: 0.8
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Child
  extends: upper-case-trope
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].tension_level,
        Some(0.8),
        "case-insensitive slug matching should work"
    );
}

// ═══════════════════════════════════════════════════════════
// Pass-through: world tropes without extends
// ═══════════════════════════════════════════════════════════

#[test]
fn world_trope_without_extends_passes_through_unchanged() {
    let genre = tropes_from_yaml(
        r#"
- name: Abstract Parent
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Standalone Trope
  category: revelation
  triggers:
    - something happens
  tension_level: 0.5
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(resolved.len(), 1);
    let standalone = &resolved[0];
    assert_eq!(standalone.name.as_str(), "Standalone Trope");
    assert_eq!(standalone.category, "revelation");
    assert_eq!(standalone.tension_level, Some(0.5));
}

// ═══════════════════════════════════════════════════════════
// Abstract flag and extends cleared after resolution
// ═══════════════════════════════════════════════════════════

#[test]
fn resolved_trope_is_not_abstract() {
    let genre = tropes_from_yaml(
        r#"
- name: Abstract Parent
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Concrete Child
  extends: abstract-parent
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert!(
        !resolved[0].is_abstract,
        "resolved trope must not be abstract"
    );
}

#[test]
fn resolved_trope_has_no_extends() {
    let genre = tropes_from_yaml(
        r#"
- name: Parent
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Child
  extends: parent
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert!(
        resolved[0].extends.is_none(),
        "extends should be cleared after resolution"
    );
}

// ═══════════════════════════════════════════════════════════
// Only world tropes in output (abstract parents not emitted)
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_abstract_tropes_not_in_output() {
    let genre = tropes_from_yaml(
        r#"
- name: Abstract One
  abstract: true
  category: recurring
- name: Abstract Two
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Concrete
  extends: abstract-one
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved.len(),
        1,
        "only world tropes should appear in output"
    );
    assert_eq!(resolved[0].name.as_str(), "Concrete");
}

// ═══════════════════════════════════════════════════════════
// ID inheritance
// ═══════════════════════════════════════════════════════════

#[test]
fn merge_inherits_id_from_parent_when_child_has_none() {
    let genre = tropes_from_yaml(
        r#"
- name: Parent
  id: parent_id
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Child
  extends: parent
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].id.as_deref(),
        Some("parent_id"),
        "child should inherit parent's id when absent"
    );
}

#[test]
fn merge_child_id_overrides_parent_id() {
    let genre = tropes_from_yaml(
        r#"
- name: Parent
  id: parent_id
  abstract: true
  category: conflict
"#,
    );
    let world = tropes_from_yaml(
        r#"
- name: Child
  id: child_id
  extends: parent
"#,
    );

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &world).unwrap();
    assert_eq!(
        resolved[0].id.as_deref(),
        Some("child_id"),
        "child id should override parent id"
    );
}

// ═══════════════════════════════════════════════════════════
// Empty inputs
// ═══════════════════════════════════════════════════════════

#[test]
fn empty_genre_and_world_tropes_returns_empty() {
    let resolved = sidequest_genre::resolve_trope_inheritance(&[], &[]).unwrap();
    assert!(resolved.is_empty());
}

#[test]
fn empty_world_tropes_returns_empty() {
    let genre = tropes_from_yaml(
        r#"
- name: Abstract Only
  abstract: true
  category: conflict
"#,
    );
    let resolved = sidequest_genre::resolve_trope_inheritance(&genre, &[]).unwrap();
    assert!(resolved.is_empty(), "no world tropes → no output");
}

// ═══════════════════════════════════════════════════════════
// Rule #8: NonBlankString rejects empty via deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn trope_name_rejects_empty_string_via_serde() {
    let yaml = r#"
- name: ""
  category: conflict
"#;
    let result: Result<Vec<TropeDefinition>, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "TropeDefinition with empty name should fail deserialization (NonBlankString validation)"
    );
}

#[test]
fn trope_name_rejects_whitespace_only_via_serde() {
    let yaml = r#"
- name: "   "
  category: conflict
"#;
    let result: Result<Vec<TropeDefinition>, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "TropeDefinition with whitespace-only name should fail deserialization"
    );
}

// ═══════════════════════════════════════════════════════════
// Rule #8: ScenarioPack name rejects empty via deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn scenario_pack_name_rejects_empty_via_serde() {
    let yaml = r#"
name: ""
version: "1.0"
description: A scenario
duration_minutes: 60
max_players: 4
player_roles: []
pacing:
  scene_budget: 10
  acts: []
"#;
    let result: Result<ScenarioPack, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "ScenarioPack with empty name should fail deserialization"
    );
}

// ═══════════════════════════════════════════════════════════
// deny_unknown_fields on ScenarioPack
// ═══════════════════════════════════════════════════════════

#[test]
fn scenario_pack_rejects_unknown_fields() {
    let yaml = r#"
name: Valid Name
version: "1.0"
description: A scenario
duration_minutes: 60
max_players: 4
player_roles: []
pacing:
  scene_budget: 10
  acts: []
unknown_field: should fail
"#;
    let result: Result<ScenarioPack, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "ScenarioPack should reject unknown fields");
}

// ═══════════════════════════════════════════════════════════
// ScenarioPack minimal valid deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn scenario_pack_minimal_deserializes() {
    let yaml = r#"
name: Test Scenario
version: "1.0"
description: A test
duration_minutes: 120
max_players: 6
player_roles:
  - id: detective
    archetype_hint: Keen observer
    narrative_position: Lead investigator
pacing:
  scene_budget: 15
  acts:
    - id: act_1
      name: Setup
      scenes: 5
      trope_range: [0.0, 0.33]
      narrator_tone: mysterious
"#;
    let scenario: ScenarioPack = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(scenario.name.as_str(), "Test Scenario");
    assert_eq!(scenario.duration_minutes, 120);
    assert_eq!(scenario.max_players, 6);
    assert_eq!(scenario.player_roles.len(), 1);
    assert_eq!(scenario.pacing.acts.len(), 1);
    assert_eq!(scenario.pacing.scene_budget, 15);
    // Optional fields default to empty
    assert!(scenario.assignment_matrix.suspects.is_empty());
    assert!(scenario.clue_graph.nodes.is_empty());
    assert!(scenario.atmosphere_matrix.variants.is_empty());
    assert!(scenario.npcs.is_empty());
}
