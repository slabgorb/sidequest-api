//! Story 10-2: Genre archetype OCEAN baselines — default profiles per NPC
//! archetype in genre pack, random generation for unnamed NPCs.
//!
//! RED phase: tests compile but FAIL because the implementation does not exist yet.
//!
//! Acceptance criteria:
//!   AC-1: Genre pack archetype YAML supports optional OCEAN baseline
//!   AC-2: Archetypes without OCEAN fall back to neutral (5.0 all)
//!   AC-3: NPC created from archetype inherits OCEAN with jitter
//!   AC-4: Random NPC generation produces valid profiles (0.0–10.0)
//!   AC-5: At least one genre pack has OCEAN baselines (mutant_wasteland)

use sidequest_game::OceanProfile;

// ─── Helpers ──────────────────────────────────────────────

/// Archetype YAML with an OCEAN baseline block.
const ARCHETYPE_YAML_WITH_OCEAN: &str = r#"
name: Wasteland Raider
description: A violent scavenger
personality_traits:
  - aggressive
  - cunning
typical_classes:
  - Scavenger
typical_races:
  - Mutant Human
stat_ranges:
  Toughness: [12, 16]
inventory_hints:
  - rusty machete
dialogue_quirks:
  - growls threats
disposition_default: -15
ocean:
  openness: 3.0
  conscientiousness: 2.5
  extraversion: 7.0
  agreeableness: 1.5
  neuroticism: 8.0
"#;

/// Archetype YAML without an OCEAN section — should still deserialize.
const ARCHETYPE_YAML_NO_OCEAN: &str = r#"
name: Village Elder
description: The leader of a small settlement
personality_traits:
  - cautious
  - protective
typical_classes:
  - Pureblood
typical_races:
  - Mutant Human
stat_ranges:
  Wits: [14, 18]
inventory_hints:
  - a staff carved from irradiated wood
dialogue_quirks:
  - refers to the apocalypse by a local myth name
disposition_default: 0
"#;

// ─── AC-1: Archetype YAML supports optional OCEAN baseline ──

#[test]
fn archetype_with_ocean_baseline_deserializes() {
    // Deserialize to Value to check that the ocean field is accepted and preserved.
    let val: serde_json::Value = serde_yaml::from_str(ARCHETYPE_YAML_WITH_OCEAN).unwrap();
    let ocean = val.get("ocean").expect("YAML should have ocean key");
    assert!((ocean["openness"].as_f64().unwrap() - 3.0).abs() < f64::EPSILON);
    assert!((ocean["neuroticism"].as_f64().unwrap() - 8.0).abs() < f64::EPSILON);

    // Now verify the typed struct also accepts it — this is the real test.
    // NpcArchetype must have an `ocean` field to not reject the key.
    let archetype: sidequest_genre::NpcArchetype =
        serde_yaml::from_str(ARCHETYPE_YAML_WITH_OCEAN).unwrap();
    // Serialize back and check ocean survives the round-trip through the typed struct.
    let json = serde_json::to_value(&archetype).unwrap();
    let ocean = json.get("ocean").expect(
        "NpcArchetype should preserve ocean field after deserialization — \
         add `ocean: Option<OceanProfile>` (or equivalent) to NpcArchetype",
    );
    assert!(
        ocean.get("openness").is_some(),
        "ocean baseline should have openness dimension"
    );
}

#[test]
fn archetype_ocean_baseline_survives_yaml_roundtrip() {
    let archetype: sidequest_genre::NpcArchetype =
        serde_yaml::from_str(ARCHETYPE_YAML_WITH_OCEAN).unwrap();
    let yaml_out = serde_yaml::to_string(&archetype).unwrap();
    let archetype2: sidequest_genre::NpcArchetype = serde_yaml::from_str(&yaml_out).unwrap();

    let json1 = serde_json::to_value(&archetype).unwrap();
    let json2 = serde_json::to_value(&archetype2).unwrap();
    let ocean1 = json1.get("ocean").expect("ocean missing on first deser");
    let ocean2 = json2.get("ocean").expect("ocean missing after round-trip");
    assert_eq!(
        ocean1, ocean2,
        "OCEAN baseline should survive YAML round-trip"
    );
}

// ─── AC-2: Archetypes without OCEAN fall back to neutral ────

#[test]
fn archetype_without_ocean_still_deserializes() {
    // Must not reject YAML that omits the ocean key.
    let archetype: sidequest_genre::NpcArchetype =
        serde_yaml::from_str(ARCHETYPE_YAML_NO_OCEAN).unwrap();
    assert_eq!(archetype.name.as_str(), "Village Elder");
}

#[test]
fn archetype_without_ocean_has_no_ocean_in_json() {
    let archetype: sidequest_genre::NpcArchetype =
        serde_yaml::from_str(ARCHETYPE_YAML_NO_OCEAN).unwrap();
    let json = serde_json::to_value(&archetype).unwrap();
    // When no ocean is in the YAML, it should be absent or null.
    let ocean = json.get("ocean");
    assert!(
        ocean.is_none() || ocean.unwrap().is_null(),
        "archetype without ocean YAML should have null/absent ocean, got: {ocean:?}"
    );
}

#[test]
fn archetype_missing_ocean_effective_baseline_is_neutral() {
    // Application logic: when archetype has no ocean, the effective baseline
    // for NPC creation should be Default (5.0 all). We test that OceanProfile::default()
    // provides this — the integration is that NPC creation uses unwrap_or_default().
    let neutral = OceanProfile::default();
    assert!((neutral.openness - 5.0).abs() < f64::EPSILON);
    assert!((neutral.conscientiousness - 5.0).abs() < f64::EPSILON);
    assert!((neutral.extraversion - 5.0).abs() < f64::EPSILON);
    assert!((neutral.agreeableness - 5.0).abs() < f64::EPSILON);
    assert!((neutral.neuroticism - 5.0).abs() < f64::EPSILON);
}

// ─── AC-3: NPC from archetype inherits OCEAN with jitter ────

#[test]
fn ocean_profile_with_jitter_stays_close_to_baseline() {
    let baseline = OceanProfile {
        openness: 3.0,
        conscientiousness: 2.5,
        extraversion: 7.0,
        agreeableness: 1.5,
        neuroticism: 8.0,
    };
    // with_jitter should produce a profile where each dimension is within ±2.0
    // of the baseline (and still clamped to 0.0–10.0).
    let jittered = baseline.with_jitter(2.0);
    let max_delta = 2.0 + f64::EPSILON;
    assert!(
        (jittered.openness - baseline.openness).abs() <= max_delta,
        "openness jitter too large: base={}, got={}",
        baseline.openness,
        jittered.openness
    );
    assert!(
        (jittered.conscientiousness - baseline.conscientiousness).abs() <= max_delta,
        "conscientiousness jitter too large"
    );
    assert!(
        (jittered.extraversion - baseline.extraversion).abs() <= max_delta,
        "extraversion jitter too large"
    );
    assert!(
        (jittered.agreeableness - baseline.agreeableness).abs() <= max_delta,
        "agreeableness jitter too large"
    );
    assert!(
        (jittered.neuroticism - baseline.neuroticism).abs() <= max_delta,
        "neuroticism jitter too large"
    );
}

#[test]
fn ocean_profile_with_jitter_respects_bounds() {
    // Edge case: baseline near 0.0 — jitter must not go negative.
    let low = OceanProfile {
        openness: 0.5,
        conscientiousness: 0.5,
        extraversion: 0.5,
        agreeableness: 0.5,
        neuroticism: 0.5,
    };
    for _ in 0..20 {
        let j = low.with_jitter(2.0);
        assert!(
            j.openness >= 0.0 && j.openness <= 10.0,
            "out of bounds: {}",
            j.openness
        );
        assert!(j.conscientiousness >= 0.0 && j.conscientiousness <= 10.0);
        assert!(j.extraversion >= 0.0 && j.extraversion <= 10.0);
        assert!(j.agreeableness >= 0.0 && j.agreeableness <= 10.0);
        assert!(j.neuroticism >= 0.0 && j.neuroticism <= 10.0);
    }

    // Edge case: baseline near 10.0 — jitter must not exceed 10.0.
    let high = OceanProfile {
        openness: 9.5,
        conscientiousness: 9.5,
        extraversion: 9.5,
        agreeableness: 9.5,
        neuroticism: 9.5,
    };
    for _ in 0..20 {
        let j = high.with_jitter(2.0);
        assert!(
            j.openness >= 0.0 && j.openness <= 10.0,
            "out of bounds: {}",
            j.openness
        );
        assert!(j.neuroticism >= 0.0 && j.neuroticism <= 10.0);
    }
}

#[test]
fn ocean_profile_with_jitter_is_not_identical() {
    let baseline = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    // Run enough times that at least one dimension should differ.
    let mut any_different = false;
    for _ in 0..50 {
        let j = baseline.with_jitter(1.0);
        if (j.openness - 5.0).abs() > f64::EPSILON
            || (j.conscientiousness - 5.0).abs() > f64::EPSILON
            || (j.extraversion - 5.0).abs() > f64::EPSILON
            || (j.agreeableness - 5.0).abs() > f64::EPSILON
            || (j.neuroticism - 5.0).abs() > f64::EPSILON
        {
            any_different = true;
            break;
        }
    }
    assert!(
        any_different,
        "with_jitter should produce variation, not identical copies"
    );
}

// ─── AC-4: Random NPC generation produces valid profiles ────

#[test]
fn ocean_profile_random_values_in_range() {
    // OceanProfile::random() should produce a valid profile with all values in [0.0, 10.0].
    for _ in 0..100 {
        let profile = OceanProfile::random();
        assert!(
            profile.openness >= 0.0 && profile.openness <= 10.0,
            "random openness out of range: {}",
            profile.openness
        );
        assert!(
            profile.conscientiousness >= 0.0 && profile.conscientiousness <= 10.0,
            "random conscientiousness out of range: {}",
            profile.conscientiousness
        );
        assert!(
            profile.extraversion >= 0.0 && profile.extraversion <= 10.0,
            "random extraversion out of range: {}",
            profile.extraversion
        );
        assert!(
            profile.agreeableness >= 0.0 && profile.agreeableness <= 10.0,
            "random agreeableness out of range: {}",
            profile.agreeableness
        );
        assert!(
            profile.neuroticism >= 0.0 && profile.neuroticism <= 10.0,
            "random neuroticism out of range: {}",
            profile.neuroticism
        );
    }
}

#[test]
fn ocean_profile_random_produces_variety() {
    // Random generation should not always produce the same profile.
    let profiles: Vec<OceanProfile> = (0..20).map(|_| OceanProfile::random()).collect();
    let first = &profiles[0];
    let any_different = profiles[1..].iter().any(|p| {
        (p.openness - first.openness).abs() > f64::EPSILON
            || (p.extraversion - first.extraversion).abs() > f64::EPSILON
    });
    assert!(any_different, "random() should produce varied profiles");
}

// ─── AC-5: Integration — mutant_wasteland has OCEAN baselines ──

#[test]
fn mutant_wasteland_has_archetype_ocean_baselines() {
    // The genre packs live in the content repo — resolve relative to workspace root.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let content_path = workspace_root
        .parent()
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs")
        .join("mutant_wasteland")
        .join("archetypes.yaml");

    if !content_path.exists() {
        // Skip gracefully in CI or if content repo is not checked out.
        eprintln!(
            "SKIP: mutant_wasteland archetypes.yaml not found at {}",
            content_path.display()
        );
        return;
    }

    let content = std::fs::read_to_string(&content_path).unwrap();

    // Deserialize as Value first — if the YAML has ocean keys the struct must accept them.
    let raw: Vec<serde_json::Value> = serde_yaml::from_str(&content).unwrap();
    let with_ocean_in_yaml = raw.iter().filter(|a| a.get("ocean").is_some()).count();
    assert!(
        with_ocean_in_yaml > 0,
        "at least one mutant_wasteland archetype should have an OCEAN baseline in YAML, \
         but none of {} archetypes have one — add ocean sections to archetypes.yaml",
        raw.len()
    );

    // Also verify the typed struct accepts the YAML with ocean keys.
    let archetypes: Vec<sidequest_genre::NpcArchetype> = serde_yaml::from_str(&content).unwrap();
    assert_eq!(
        archetypes.len(),
        raw.len(),
        "typed deser should load same count"
    );
}
