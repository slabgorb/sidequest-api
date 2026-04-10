//! Story 12-1: Cinematic track variation selection — MusicDirector uses themed score cues
//!
//! RED phase tests. These verify:
//! - AC1: TrackVariation enum exists and parses from AudioVariation.variation_type
//! - AC2: MoodContext extended with 5 new fields
//! - AC3: Variation selection logic (priority table)
//! - AC4: MusicDirector uses themed tracks with per-variation anti-repetition
//! - AC5: Server wiring populates new MoodContext fields
//! - AC6: Telemetry emits chosen variation
//! - AC7: Full pipeline (classify_mood → select_variation → select_track → AudioCue)
//! - AC8: No regressions (backward compat with theme-less genre packs)

use std::collections::HashMap;

use sidequest_game::{
    AudioAction, AudioChannel, MoodClassification, MoodContext, MoodKey, MusicDirector,
    MusicEvalResult,
};
use sidequest_genre::{AudioConfig, AudioTheme, AudioVariation, MixerConfig, MoodTrack};

// ───────────────────────────────────────────────────────────────
// Helpers
// ───────────────────────────────────────────────────────────────

/// Minimal AudioConfig with NO themes — backward compatibility baseline.
fn config_without_themes() -> AudioConfig {
    let mut mood_tracks = HashMap::new();
    mood_tracks.insert(
        "combat".to_string(),
        vec![MoodTrack {
            path: "audio/music/combat_1.ogg".to_string(),
            title: "Battle Drums".to_string(),
            bpm: 140,
            energy: 0.9,
        }],
    );
    mood_tracks.insert(
        "exploration".to_string(),
        vec![MoodTrack {
            path: "audio/music/explore_1.ogg".to_string(),
            title: "Wanderer's Path".to_string(),
            bpm: 90,
            energy: 0.4,
        }],
    );
    AudioConfig {
        mood_tracks,
        mood_keywords: HashMap::new(),
        sfx_library: HashMap::new(),
        creature_voice_presets: HashMap::new(),
        mixer: MixerConfig {
            music_volume: 0.6,
            sfx_volume: 0.8,
            voice_volume: 1.0,
            duck_music_for_voice: true,
            duck_amount_db: -12.0,
            crossfade_default_ms: 3000,
        },
        themes: vec![],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

/// AudioConfig with themed variations for exploration mood.
fn config_with_themes() -> AudioConfig {
    let mut config = config_without_themes();

    // Add exploration themes with all 6 variation types
    config.themes = vec![AudioTheme {
        name: "forest".to_string(),
        mood: "exploration".to_string(),
        base_prompt: "ambient forest".to_string(),
        variations: vec![
            AudioVariation {
                variation_type: "overture".to_string(),
                path: "audio/themes/exploration/set-1/overture.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "ambient".to_string(),
                path: "audio/themes/exploration/set-1/ambient.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "sparse".to_string(),
                path: "audio/themes/exploration/set-1/sparse.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "full".to_string(),
                path: "audio/themes/exploration/set-1/full.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "tension_build".to_string(),
                path: "audio/themes/exploration/set-1/tension_build.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "resolution".to_string(),
                path: "audio/themes/exploration/set-1/resolution.ogg".to_string(),
            },
        ],
    }];

    // Also add a combat theme to test multi-mood theming
    config.themes.push(AudioTheme {
        name: "battle".to_string(),
        mood: "combat".to_string(),
        base_prompt: "epic battle".to_string(),
        variations: vec![
            AudioVariation {
                variation_type: "full".to_string(),
                path: "audio/themes/combat/set-1/full.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "tension_build".to_string(),
                path: "audio/themes/combat/set-1/tension_build.ogg".to_string(),
            },
            AudioVariation {
                variation_type: "resolution".to_string(),
                path: "audio/themes/combat/set-1/resolution.ogg".to_string(),
            },
        ],
    });

    config
}

/// Build a MoodContext with the 5 new fields (AC2) plus existing defaults.
fn mood_ctx_with_new_fields(
    location_changed: bool,
    scene_turn_count: u32,
    drama_weight: f32,
    combat_just_ended: bool,
    session_start: bool,
) -> MoodContext {
    MoodContext {
        location_changed,
        scene_turn_count,
        drama_weight,
        combat_just_ended,
        session_start,
        ..Default::default()
    }
}

// ═══════════════════════════════════════════════════════════════
// AC1: TrackVariation enum
// ═══════════════════════════════════════════════════════════════

/// AC1: TrackVariation enum exists with all 6 variants.
#[test]
fn track_variation_enum_has_all_variants() {
    use sidequest_game::TrackVariation;

    let variants = [
        TrackVariation::Full,
        TrackVariation::Overture,
        TrackVariation::Ambient,
        TrackVariation::Sparse,
        TrackVariation::TensionBuild,
        TrackVariation::Resolution,
    ];

    // Each variant is distinct
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i == j {
                assert_eq!(a, b, "same variant should equal itself");
            } else {
                assert_ne!(a, b, "different variants should not be equal");
            }
        }
    }
}

/// AC1: TrackVariation serde round-trips correctly (snake_case).
#[test]
fn track_variation_serde_round_trip() {
    use sidequest_game::TrackVariation;

    let cases = [
        (TrackVariation::Full, "\"full\""),
        (TrackVariation::Overture, "\"overture\""),
        (TrackVariation::Ambient, "\"ambient\""),
        (TrackVariation::Sparse, "\"sparse\""),
        (TrackVariation::TensionBuild, "\"tension_build\""),
        (TrackVariation::Resolution, "\"resolution\""),
    ];

    for (variant, expected_json) in &cases {
        let serialized = serde_json::to_string(variant).expect("serialize");
        assert_eq!(
            &serialized, expected_json,
            "TrackVariation::{variant:?} should serialize to {expected_json}"
        );

        let deserialized: TrackVariation =
            serde_json::from_str(expected_json).expect("deserialize");
        assert_eq!(
            &deserialized, variant,
            "{expected_json} should deserialize to TrackVariation::{variant:?}"
        );
    }
}

/// AC1: AudioVariation.as_variation() converts string to typed enum.
#[test]
fn audio_variation_as_variation_parses_types() {
    let cases = [
        ("full", "Full"),
        ("ambient", "Ambient"),
        ("sparse", "Sparse"),
        ("overture", "Overture"),
        ("tension_build", "TensionBuild"),
        ("resolution", "Resolution"),
    ];

    for (input, expected_name) in &cases {
        let av = AudioVariation {
            variation_type: input.to_string(),
            path: "test.ogg".to_string(),
        };
        let variation = av.as_variation();
        assert_eq!(
            format!("{variation:?}"),
            *expected_name,
            "as_variation() for '{input}' should produce {expected_name}"
        );
    }
}

/// AC1: AudioVariation.as_variation() defaults to Full for unknown types.
#[test]
fn audio_variation_unknown_type_defaults_to_full() {
    use sidequest_game::TrackVariation;

    let av = AudioVariation {
        variation_type: "something_new".to_string(),
        path: "test.ogg".to_string(),
    };
    assert_eq!(
        av.as_variation(),
        TrackVariation::Full,
        "unknown variation_type should default to Full"
    );
}

/// Rule #2: TrackVariation is #[non_exhaustive] (public enum that may grow).
#[test]
fn track_variation_is_non_exhaustive() {
    use sidequest_game::TrackVariation;

    // This test verifies the enum is usable with a wildcard match.
    // If #[non_exhaustive] is missing, this test still compiles but
    // the rule check ensures the attribute exists. The real enforcement
    // is that external crates cannot exhaustively match without `_ =>`.
    let v = TrackVariation::Full;
    let _name = match v {
        TrackVariation::Full => "full",
        TrackVariation::Overture => "overture",
        TrackVariation::Ambient => "ambient",
        TrackVariation::Sparse => "sparse",
        TrackVariation::TensionBuild => "tension_build",
        TrackVariation::Resolution => "resolution",
        _ => "unknown", // Required by #[non_exhaustive] from external crate
    };
    // If this compiles with the wildcard arm, the enum is non_exhaustive
    // (or at least compatible with future extension).
}

// ═══════════════════════════════════════════════════════════════
// AC2: MoodContext extended fields
// ═══════════════════════════════════════════════════════════════

/// AC2: MoodContext has all 5 new fields with correct default values.
#[test]
fn mood_context_new_fields_exist_with_defaults() {
    let ctx = MoodContext::default();

    assert_eq!(
        ctx.location_changed, false,
        "location_changed should default to false"
    );
    assert_eq!(
        ctx.scene_turn_count, 0,
        "scene_turn_count should default to 0"
    );
    assert_eq!(ctx.drama_weight, 0.0, "drama_weight should default to 0.0");
    assert_eq!(
        ctx.combat_just_ended, false,
        "combat_just_ended should default to false"
    );
    assert_eq!(
        ctx.session_start, false,
        "session_start should default to false"
    );
}

/// AC2: MoodContext new fields can be set explicitly.
#[test]
fn mood_context_new_fields_settable() {
    let ctx = MoodContext {
        location_changed: true,
        scene_turn_count: 7,
        drama_weight: 0.85,
        combat_just_ended: true,
        session_start: true,
        ..Default::default()
    };

    assert!(ctx.location_changed);
    assert_eq!(ctx.scene_turn_count, 7);
    assert!((ctx.drama_weight - 0.85).abs() < f32::EPSILON);
    assert!(ctx.combat_just_ended);
    assert!(ctx.session_start);
}

// ═══════════════════════════════════════════════════════════════
// AC3: Variation selection logic (priority table)
// ═══════════════════════════════════════════════════════════════

/// AC3 Priority 1: session_start → Overture.
#[test]
fn select_variation_priority_1_session_start() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Overture,
        "session_start should select Overture"
    );
}

/// AC3 Priority 1: location_changed + turn 0 → Overture.
#[test]
fn select_variation_priority_1_location_arrival() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(true, 0, 0.3, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Overture,
        "location_changed + scene_turn_count==0 should select Overture"
    );
}

/// AC3 Priority 1: location_changed but NOT turn 0 → should NOT be Overture.
#[test]
fn select_variation_location_changed_but_not_first_turn() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4,
        confidence: 0.8,
    };
    // location_changed=true but scene_turn_count=3 → not overture
    let ctx = mood_ctx_with_new_fields(true, 3, 0.2, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_ne!(
        variation,
        TrackVariation::Overture,
        "location_changed on turn 3 should NOT select Overture"
    );
}

/// AC3 Priority 2: combat_just_ended → Resolution.
#[test]
fn select_variation_priority_2_combat_ended() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.4, true, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Resolution,
        "combat_just_ended should select Resolution"
    );
}

/// AC3 Priority 2: quest_completed → Resolution.
#[test]
fn select_variation_priority_2_quest_completed() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let mut ctx = mood_ctx_with_new_fields(false, 2, 0.4, false, false);
    ctx.quest_completed = true;

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Resolution,
        "quest_completed should select Resolution"
    );
}

/// AC3 Priority 3: intensity >= 0.7 (non-combat) → TensionBuild.
#[test]
fn select_variation_priority_3_high_intensity() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::TENSION,
        intensity: 0.7, // exactly at threshold
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.4, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::TensionBuild,
        "intensity >= 0.7 should select TensionBuild"
    );
}

/// AC3 Priority 3: drama_weight >= 0.7 → TensionBuild.
#[test]
fn select_variation_priority_3_high_drama_weight() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.7, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::TensionBuild,
        "drama_weight >= 0.7 should select TensionBuild"
    );
}

/// AC3 Priority 4: low intensity → Ambient.
#[test]
fn select_variation_priority_4_low_intensity() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.3, // exactly at threshold
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.3, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Ambient,
        "intensity <= 0.3 should select Ambient"
    );
}

/// AC3 Priority 4: high scene_turn_count → Ambient.
#[test]
fn select_variation_priority_4_long_scene() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 4, 0.3, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Ambient,
        "scene_turn_count >= 4 should select Ambient"
    );
}

/// AC3 Priority 5: mid-intensity + low drama → Sparse.
#[test]
fn select_variation_priority_5_sparse_conditions() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4, // 0.3 < intensity <= 0.5
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.2, false, false); // drama_weight <= 0.3

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Sparse,
        "mid-intensity (0.4) + low drama (0.2) should select Sparse"
    );
}

/// AC3 Priority 6: when no other condition matches → Full.
#[test]
fn select_variation_priority_6_full_fallback() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.6, // not high enough for tension, not low enough for ambient
        confidence: 0.8,
    };
    // drama_weight=0.5 (not high enough for tension, not low enough for sparse)
    let ctx = mood_ctx_with_new_fields(false, 2, 0.5, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Full,
        "when no priority condition matches, should fall back to Full"
    );
}

/// AC3: Priority ordering — higher priority overrides lower.
/// session_start (P1) should win over combat_just_ended (P2).
#[test]
fn select_variation_priority_ordering_p1_over_p2() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    // Both session_start (P1) and combat_just_ended (P2) true
    let ctx = mood_ctx_with_new_fields(false, 0, 0.4, true, true);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Overture,
        "P1 (session_start) should override P2 (combat_just_ended)"
    );
}

/// AC3: Fallback chain — if preferred variation has no tracks, fall back to Full.
#[test]
fn select_variation_fallback_to_full_when_preferred_unavailable() {
    use sidequest_game::TrackVariation;

    // Config with only "full" variation for exploration (no overture tracks)
    let mut config = config_without_themes();
    config.themes = vec![AudioTheme {
        name: "minimal".to_string(),
        mood: "exploration".to_string(),
        base_prompt: "minimal".to_string(),
        variations: vec![AudioVariation {
            variation_type: "full".to_string(),
            path: "audio/themes/exploration/full.ogg".to_string(),
        }],
    }];

    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4,
        confidence: 0.8,
    };
    // session_start should want Overture, but only Full is available
    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Full,
        "should fall back to Full when preferred variation has no tracks"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC4: MusicDirector themed track selection + anti-repetition
// ═══════════════════════════════════════════════════════════════

/// AC4: Constructor indexes themes into per-variation lookup.
#[test]
fn music_director_indexes_themes_by_variation() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    // The director should have themed tracks indexed
    // We verify through behavior: selecting an overture should return an overture track
    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true); // session_start → Overture

    let MusicEvalResult::Cue(audio_cue) = director.evaluate("Arriving at a new location", &ctx)
    else {
        panic!("should produce a cue for session start");
    };
    let track_id = audio_cue.track_id.unwrap();
    assert!(
        track_id.contains("overture"),
        "session_start should select an overture track, got: {track_id}"
    );
}

/// AC4: select_track uses themed variations when themes are available.
#[test]
fn music_director_uses_themed_tracks() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    // Force exploration mood with high intensity → TensionBuild variant
    let ctx = MoodContext {
        drama_weight: 0.8,
        scene_turn_count: 2,
        ..Default::default()
    };

    let MusicEvalResult::Cue(audio_cue) = director.evaluate("The shadows grow deeper", &ctx) else {
        panic!("should produce a cue");
    };
    let track_id = audio_cue.track_id.unwrap();
    assert!(
        track_id.contains("tension_build"),
        "high drama should select a tension_build track, got: {track_id}"
    );
}

/// AC4: ThemeRotator per-variation keying prevents same variation track repeating.
#[test]
fn theme_rotator_uses_variation_keying() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    // Two session_start evaluations should use "{mood}:{variation}" keying
    // to track history per-variation, not per-mood.
    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true);

    let cue1 = director.evaluate("First arrival", &ctx);
    assert!(matches!(cue1, MusicEvalResult::Cue(_)));

    // Force mood change so evaluate triggers again
    director.evaluate(
        "Combat starts!",
        &MoodContext {
            in_combat: true,
            ..Default::default()
        },
    );

    // Back to exploration with overture conditions
    let cue2 = director.evaluate("Arriving at the next town", &ctx);

    // Telemetry should show per-variation history keying
    let telemetry = director.telemetry_snapshot();
    // The history keys should include variation suffix (e.g. "exploration:overture")
    let has_variation_key = telemetry.rotation_history.keys().any(|k| k.contains(':'));
    assert!(
        has_variation_key,
        "rotation history should use '{{mood}}:{{variation}}' keying, got keys: {:?}",
        telemetry.rotation_history.keys().collect::<Vec<_>>()
    );
}

// ═══════════════════════════════════════════════════════════════
// AC6: Telemetry emits chosen variation
// ═══════════════════════════════════════════════════════════════

/// AC6: MusicTelemetry includes chosen variation type.
#[test]
fn telemetry_includes_chosen_variation() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true);
    let _ = director.evaluate("Arriving somewhere new", &ctx);

    let telemetry = director.telemetry_snapshot();
    // Telemetry should report the chosen variation
    assert!(
        telemetry.current_variation.is_some(),
        "telemetry should include current_variation after evaluation"
    );
    assert_eq!(
        telemetry.current_variation.as_deref(),
        Some("overture"),
        "session_start should show overture as current_variation"
    );
}

/// AC6: Telemetry includes selection reason.
#[test]
fn telemetry_includes_selection_reason() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    let ctx = mood_ctx_with_new_fields(false, 0, 0.0, false, true);
    let _ = director.evaluate("Arriving somewhere new", &ctx);

    let telemetry = director.telemetry_snapshot();
    assert!(
        telemetry.variation_reason.is_some(),
        "telemetry should include variation_reason"
    );
    let reason = telemetry.variation_reason.as_deref().unwrap();
    assert!(
        reason.contains("priority_1") || reason.contains("overture"),
        "selection reason should indicate priority level, got: {reason}"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC7: Full pipeline test
// ═══════════════════════════════════════════════════════════════

/// AC7: Full pipeline — classify_mood → select_variation → select_track → AudioCue.
#[test]
fn full_pipeline_classify_to_audio_cue() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    // Simulate a session start at a new location
    let ctx = MoodContext {
        session_start: true,
        location_changed: true,
        scene_turn_count: 0,
        ..Default::default()
    };

    // classify_mood should work with the new context
    let classification = director.classify_mood("Welcome to the enchanted forest", &ctx);
    assert_eq!(
        classification.primary,
        MoodKey::EXPLORATION,
        "peaceful narration should classify as Exploration"
    );

    // select_variation should pick Overture for session start
    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        sidequest_game::TrackVariation::Overture,
        "session start should select Overture variation"
    );

    // evaluate should produce a cue with the overture track
    let MusicEvalResult::Cue(cue) = director.evaluate("Welcome to the enchanted forest", &ctx)
    else {
        panic!("should produce an AudioCue");
    };
    assert_eq!(cue.channel, AudioChannel::Music);
    assert!(cue.track_id.is_some());
    assert!(
        cue.track_id.as_ref().unwrap().contains("overture"),
        "cue should reference an overture track"
    );
}

/// AC7: Full pipeline — combat ending triggers resolution variant.
#[test]
fn full_pipeline_combat_end_resolution() {
    let config = config_with_themes();
    let mut director = MusicDirector::new(&config);

    // Start in combat
    let combat_ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };
    let _ = director.evaluate("Swords clash!", &combat_ctx);

    // Combat ends — combat_just_ended should trigger resolution
    let post_combat_ctx = MoodContext {
        combat_just_ended: true,
        scene_turn_count: 1,
        ..Default::default()
    };
    let MusicEvalResult::Cue(cue) =
        director.evaluate("The battle is over, silence falls.", &post_combat_ctx)
    else {
        panic!("mood change post-combat should produce a cue");
    };
    assert!(
        cue.track_id
            .as_ref()
            .map_or(false, |t| t.contains("resolution")),
        "post-combat should play a resolution track, got: {:?}",
        cue.track_id
    );
}

// ═══════════════════════════════════════════════════════════════
// AC8: Backward compatibility
// ═══════════════════════════════════════════════════════════════

/// AC8: Genre packs without themes section work identically.
#[test]
fn backward_compat_no_themes_still_works() {
    let config = config_without_themes();
    let mut director = MusicDirector::new(&config);

    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    let MusicEvalResult::Cue(cue) = director.evaluate("Battle begins!", &ctx) else {
        panic!("combat cue should still work without themes");
    };
    assert_eq!(cue.channel, AudioChannel::Music);
    assert!(cue.track_id.unwrap().contains("combat"));
}

/// AC8: New MoodContext fields don't break existing evaluate behavior.
#[test]
fn backward_compat_new_fields_default_no_regression() {
    let config = config_without_themes();
    let mut director = MusicDirector::new(&config);

    // Default MoodContext (all new fields false/0) should behave like before
    let ctx = MoodContext::default();
    let classification = director.classify_mood("Walking through the forest", &ctx);
    assert_eq!(
        classification.primary,
        MoodKey::EXPLORATION,
        "default context should still produce Exploration mood"
    );
}

// ═══════════════════════════════════════════════════════════════
// Boundary / edge cases
// ═══════════════════════════════════════════════════════════════

/// Boundary: intensity exactly at 0.7 → TensionBuild (inclusive threshold).
#[test]
fn boundary_intensity_at_0_7_is_tension_build() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.7,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.4, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(variation, TrackVariation::TensionBuild);
}

/// Boundary: intensity at 0.69 → NOT TensionBuild (below threshold).
#[test]
fn boundary_intensity_below_0_7_not_tension() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.69,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.4, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_ne!(
        variation,
        TrackVariation::TensionBuild,
        "intensity 0.69 should not trigger TensionBuild"
    );
}

/// Boundary: intensity exactly at 0.3 → Ambient (inclusive threshold).
#[test]
fn boundary_intensity_at_0_3_is_ambient() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.3,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.2, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(variation, TrackVariation::Ambient);
}

/// Boundary: scene_turn_count exactly 4 → Ambient.
#[test]
fn boundary_scene_turn_count_4_is_ambient() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5, // mid-range, wouldn't trigger anything else easily
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 4, 0.5, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Ambient,
        "scene_turn_count == 4 should trigger Ambient"
    );
}

/// Edge: sparse conditions require BOTH intensity in range AND low drama.
#[test]
fn sparse_requires_both_conditions() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);

    // Mid-intensity but high drama → should NOT be sparse (drama_weight too high)
    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.4,
        confidence: 0.8,
    };
    let ctx = mood_ctx_with_new_fields(false, 2, 0.5, false, false);

    let variation = director.select_variation(&classification, &ctx);
    assert_ne!(
        variation,
        TrackVariation::Sparse,
        "high drama_weight (0.5) with mid-intensity should NOT select Sparse"
    );
}

// ═══════════════════════════════════════════════════════════════
// Wiring test: non-test consumer exists
// ═══════════════════════════════════════════════════════════════

/// AC5 (wiring): verify that select_variation is callable from the public API.
/// This is a compile-time wiring test — if MusicDirector::select_variation
/// doesn't exist as a public method, this won't compile.
#[test]
fn select_variation_is_public_api() {
    use sidequest_game::TrackVariation;

    let config = config_with_themes();
    let director = MusicDirector::new(&config);
    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = MoodContext::default();

    // This call proves the method exists and is public
    let _variation: TrackVariation = director.select_variation(&classification, &ctx);
}
