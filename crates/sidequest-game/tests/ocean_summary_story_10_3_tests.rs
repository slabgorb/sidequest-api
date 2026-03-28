//! Story 10-3: OCEAN behavioral summary — scores to narrator prompt text
//!
//! RED phase: tests compile but FAIL because `behavioral_summary()` is a stub
//! returning an empty string.
//!
//! Acceptance criteria:
//!   AC-1: behavioral_summary() exists on OceanProfile and returns String
//!   AC-2: Low scores (0–3) produce low-end descriptors
//!   AC-3: High scores (7–10) produce high-end descriptors
//!   AC-4: Mid scores (3–7) are omitted from the summary
//!   AC-5: All-neutral profile produces a meaningful fallback
//!   AC-6: Output is readable natural English, not a data dump

use sidequest_game::OceanProfile;

// ─── Helpers ──────────────────────────────────────────────

/// Build a profile with one dimension set, everything else neutral (5.0).
fn profile_with(dimension: &str, value: f64) -> OceanProfile {
    let mut p = OceanProfile::default();
    match dimension {
        "openness" => p.openness = value,
        "conscientiousness" => p.conscientiousness = value,
        "extraversion" => p.extraversion = value,
        "agreeableness" => p.agreeableness = value,
        "neuroticism" => p.neuroticism = value,
        _ => panic!("unknown dimension: {dimension}"),
    }
    p
}

/// Assert that `summary` contains at least one of the given keywords
/// (case-insensitive).
fn assert_contains_any(summary: &str, keywords: &[&str], context: &str) {
    let lower = summary.to_lowercase();
    assert!(
        keywords.iter().any(|kw| lower.contains(&kw.to_lowercase())),
        "{context}: expected one of {keywords:?} in summary {summary:?}"
    );
}

/// Assert that `summary` does NOT contain any of the given keywords.
fn assert_contains_none(summary: &str, keywords: &[&str], context: &str) {
    let lower = summary.to_lowercase();
    for kw in keywords {
        assert!(
            !lower.contains(&kw.to_lowercase()),
            "{context}: did NOT expect {kw:?} in summary {summary:?}"
        );
    }
}

// ─── AC-1: behavioral_summary() exists and returns String ──

#[test]
fn behavioral_summary_returns_string() {
    let profile = OceanProfile::default();
    // Just verify it compiles and returns a String.
    let _summary: String = profile.behavioral_summary();
}

// ─── AC-2: Low scores (0–3) produce low-end descriptors ────

#[test]
fn low_openness_produces_conventional_or_practical() {
    let summary = profile_with("openness", 1.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["conventional", "practical"],
        "low openness",
    );
}

#[test]
fn low_conscientiousness_produces_spontaneous_or_flexible() {
    let summary = profile_with("conscientiousness", 2.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["spontaneous", "flexible"],
        "low conscientiousness",
    );
}

#[test]
fn low_extraversion_produces_reserved_or_quiet() {
    let summary = profile_with("extraversion", 0.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["reserved", "quiet"],
        "low extraversion",
    );
}

#[test]
fn low_agreeableness_produces_competitive_or_blunt() {
    let summary = profile_with("agreeableness", 2.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["competitive", "blunt"],
        "low agreeableness",
    );
}

#[test]
fn low_neuroticism_produces_calm_or_steady() {
    let summary = profile_with("neuroticism", 1.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["calm", "steady"],
        "low neuroticism",
    );
}

// ─── AC-3: High scores (7–10) produce high-end descriptors ──

#[test]
fn high_openness_produces_curious_or_imaginative() {
    let summary = profile_with("openness", 9.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["curious", "imaginative"],
        "high openness",
    );
}

#[test]
fn high_conscientiousness_produces_meticulous_or_disciplined() {
    let summary = profile_with("conscientiousness", 8.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["meticulous", "disciplined"],
        "high conscientiousness",
    );
}

#[test]
fn high_extraversion_produces_outgoing_or_talkative() {
    let summary = profile_with("extraversion", 7.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["outgoing", "talkative"],
        "high extraversion",
    );
}

#[test]
fn high_agreeableness_produces_cooperative_or_empathetic() {
    let summary = profile_with("agreeableness", 8.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["cooperative", "empathetic"],
        "high agreeableness",
    );
}

#[test]
fn high_neuroticism_produces_anxious_or_volatile() {
    let summary = profile_with("neuroticism", 9.5).behavioral_summary();
    assert_contains_any(
        &summary,
        &["anxious", "volatile"],
        "high neuroticism",
    );
}

// ─── AC-4: Mid scores (3–7) are omitted ────────────────────

#[test]
fn mid_openness_omits_openness_descriptors() {
    // All dimensions neutral — none of the extreme descriptors should appear.
    let summary = OceanProfile::default().behavioral_summary();
    assert_contains_none(
        &summary,
        &["conventional", "practical", "curious", "imaginative"],
        "neutral openness",
    );
}

#[test]
fn mid_extraversion_omits_extraversion_descriptors() {
    let summary = OceanProfile::default().behavioral_summary();
    assert_contains_none(
        &summary,
        &["reserved", "quiet", "outgoing", "talkative"],
        "neutral extraversion",
    );
}

#[test]
fn mixed_profile_only_includes_extreme_dimensions() {
    // Low E, high C, everything else neutral.
    let profile = OceanProfile {
        openness: 5.0,
        conscientiousness: 8.0,
        extraversion: 2.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    let summary = profile.behavioral_summary();

    // Should include low-E and high-C descriptors.
    assert_contains_any(&summary, &["reserved", "quiet"], "low E in mixed");
    assert_contains_any(&summary, &["meticulous", "disciplined"], "high C in mixed");

    // Should NOT include openness, agreeableness, or neuroticism descriptors.
    assert_contains_none(
        &summary,
        &["conventional", "practical", "curious", "imaginative"],
        "neutral O in mixed",
    );
    assert_contains_none(
        &summary,
        &["competitive", "blunt", "cooperative", "empathetic"],
        "neutral A in mixed",
    );
    assert_contains_none(
        &summary,
        &["calm", "steady", "anxious", "volatile"],
        "neutral N in mixed",
    );
}

// ─── AC-5: All-neutral profile produces meaningful fallback ──

#[test]
fn all_neutral_profile_produces_nonempty_summary() {
    let summary = OceanProfile::default().behavioral_summary();
    assert!(
        !summary.trim().is_empty(),
        "all-neutral profile should produce a non-empty summary, got empty string"
    );
}

#[test]
fn all_neutral_profile_contains_balanced_or_unremarkable() {
    let summary = OceanProfile::default().behavioral_summary();
    assert_contains_any(
        &summary,
        &["balanced", "unremarkable", "even-tempered", "moderate"],
        "all-neutral fallback",
    );
}

// ─── AC-6: Output is readable ──────────────────────────────

#[test]
fn summary_does_not_contain_raw_numbers() {
    let profile = OceanProfile {
        openness: 1.0,
        conscientiousness: 9.0,
        extraversion: 2.0,
        agreeableness: 8.0,
        neuroticism: 0.5,
    };
    let summary = profile.behavioral_summary();
    // Should not contain numeric scores or field names.
    assert!(
        !summary.contains("1.0") && !summary.contains("9.0") && !summary.contains("0.5"),
        "summary should not contain raw numeric scores: {summary:?}"
    );
    assert!(
        !summary.contains("openness") && !summary.contains("neuroticism"),
        "summary should not contain OCEAN field names: {summary:?}"
    );
}

#[test]
fn summary_with_multiple_extremes_uses_commas_or_and() {
    let profile = OceanProfile {
        openness: 1.0,
        conscientiousness: 9.0,
        extraversion: 2.0,
        agreeableness: 8.0,
        neuroticism: 0.5,
    };
    let summary = profile.behavioral_summary();
    // With 5 extreme dimensions, there should be joining punctuation.
    let has_joining = summary.contains(',') || summary.contains(" and ");
    assert!(
        has_joining,
        "multi-trait summary should use commas or 'and' to join: {summary:?}"
    );
}

#[test]
fn boundary_low_at_3_still_produces_descriptor() {
    // Score of exactly 3.0 should be treated as low (boundary inclusive).
    let summary = profile_with("extraversion", 3.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["reserved", "quiet"],
        "boundary low E=3.0",
    );
}

#[test]
fn boundary_high_at_7_still_produces_descriptor() {
    // Score of exactly 7.0 should be treated as high (boundary inclusive).
    let summary = profile_with("extraversion", 7.0).behavioral_summary();
    assert_contains_any(
        &summary,
        &["outgoing", "talkative"],
        "boundary high E=7.0",
    );
}
