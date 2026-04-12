//! Story 16-14: String-keyed moods + fallback aliases
//!
//! RED phase tests. Replace the hardcoded 7-variant Mood enum with string-keyed
//! moods. Genre packs declare mood_aliases in audio.yaml mapping custom moods
//! to core fallbacks. MusicDirector resolves mood strings through alias chains.
//!
//! ACs tested:
//!   AC1: Mood becomes a string-keyed type (MoodKey)
//!   AC2: Core moods (tension, calm, exploration, etc.) still work
//!   AC3: Genre packs declare mood_aliases in audio.yaml
//!   AC4: MusicDirector resolves mood string through alias chain
//!   AC5: Confrontation types declare their mood string
//!   AC6: Unknown moods fall back through alias chain or to default
//!   AC7: Backward-compatible enum→string mapping

use sidequest_game::music_director::{MoodClassification, MoodContext, MoodKey, MusicDirector};
use sidequest_genre::AudioConfig;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn default_mood_context() -> MoodContext {
    MoodContext {
        in_combat: false,
        in_chase: false,
        party_health_pct: 1.0,
        quest_completed: false,
        npc_died: false,
        encounter_mood_override: None,
        location_changed: false,
        scene_turn_count: 5,
        drama_weight: 0.3,
        combat_just_ended: false,
        session_start: false,
    }
}

fn audio_config_with_aliases(aliases: HashMap<String, String>) -> AudioConfig {
    let mut config = AudioConfig::empty();
    config.mood_aliases = aliases;
    // Add at least one track per core mood so select_track has something
    for mood in &[
        "combat",
        "exploration",
        "tension",
        "triumph",
        "sorrow",
        "mystery",
        "calm",
    ] {
        config.mood_tracks.insert(
            mood.to_string(),
            vec![sidequest_genre::MoodTrack {
                path: format!("audio/music/{mood}_full.ogg"),
                title: format!("{mood} track"),
                bpm: 100,
                energy: 0.5,
            }],
        );
    }
    config
}

// ═══════════════════════════════════════════════════════════
// AC1: Mood becomes a string-keyed type (MoodKey)
// ═══════════════════════════════════════════════════════════

#[test]
fn mood_key_from_string() {
    let mood = MoodKey::from("combat");
    assert_eq!(mood.as_str(), "combat");
}

#[test]
fn mood_key_equality() {
    let a = MoodKey::from("tension");
    let b = MoodKey::from("tension");
    assert_eq!(a, b);
}

#[test]
fn mood_key_inequality() {
    let a = MoodKey::from("combat");
    let b = MoodKey::from("calm");
    assert_ne!(a, b);
}

#[test]
fn mood_key_case_normalized() {
    // Mood keys should be lowercase
    let mood = MoodKey::from("Combat");
    assert_eq!(
        mood.as_str(),
        "combat",
        "MoodKey should normalize to lowercase"
    );
}

#[test]
fn mood_key_serde_roundtrip() {
    let mood = MoodKey::from("standoff");
    let json = serde_json::to_string(&mood).unwrap();
    let restored: MoodKey = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.as_str(), "standoff");
}

#[test]
fn mood_key_debug_and_clone() {
    let mood = MoodKey::from("mystery");
    let cloned = mood.clone();
    assert_eq!(mood, cloned);
    let _debug = format!("{:?}", mood);
}

#[test]
fn mood_key_hash_works() {
    let mut map: HashMap<MoodKey, &str> = HashMap::new();
    map.insert(MoodKey::from("combat"), "fight music");
    map.insert(MoodKey::from("calm"), "rest music");
    assert_eq!(map.get(&MoodKey::from("combat")), Some(&"fight music"));
}

// ═══════════════════════════════════════════════════════════
// AC2: Core moods still work
// ═══════════════════════════════════════════════════════════

#[test]
fn core_mood_constants_exist() {
    assert_eq!(MoodKey::COMBAT.as_str(), "combat");
    assert_eq!(MoodKey::EXPLORATION.as_str(), "exploration");
    assert_eq!(MoodKey::TENSION.as_str(), "tension");
    assert_eq!(MoodKey::TRIUMPH.as_str(), "triumph");
    assert_eq!(MoodKey::SORROW.as_str(), "sorrow");
    assert_eq!(MoodKey::MYSTERY.as_str(), "mystery");
    assert_eq!(MoodKey::CALM.as_str(), "calm");
}

#[test]
fn mood_classification_uses_mood_key() {
    let classification = MoodClassification {
        primary: MoodKey::COMBAT,
        intensity: 0.8,
        confidence: 1.0,
    };
    assert_eq!(classification.primary, MoodKey::COMBAT);
}

#[test]
fn music_director_classify_combat_still_works() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.in_combat = true;

    let classification = director.classify_mood("", &ctx);
    assert_eq!(
        classification.primary,
        MoodKey::COMBAT,
        "combat context should still classify as combat"
    );
}

#[test]
fn music_director_classify_tension_from_chase() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.in_chase = true;

    let classification = director.classify_mood("", &ctx);
    assert_eq!(classification.primary, MoodKey::TENSION);
}

#[test]
fn music_director_classify_triumph_from_quest() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.quest_completed = true;

    let classification = director.classify_mood("", &ctx);
    assert_eq!(classification.primary, MoodKey::TRIUMPH);
}

#[test]
fn music_director_classify_sorrow_from_npc_death() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.npc_died = true;

    let classification = director.classify_mood("", &ctx);
    assert_eq!(classification.primary, MoodKey::SORROW);
}

// ═══════════════════════════════════════════════════════════
// AC3: Genre packs declare mood_aliases in audio.yaml
// ═══════════════════════════════════════════════════════════

#[test]
fn audio_config_has_mood_aliases_field() {
    let yaml = r#"
mood_tracks: {}
sfx_library: {}
creature_voice_presets: {}
mixer:
  music_volume: 0.8
  sfx_volume: 0.9
  voice_volume: 1.0
  crossfade_default_ms: 500
mood_aliases:
  standoff: tension
  saloon: calm
  riding: exploration
"#;

    let config: AudioConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.mood_aliases.len(), 3);
    assert_eq!(
        config.mood_aliases.get("standoff"),
        Some(&"tension".to_string())
    );
    assert_eq!(config.mood_aliases.get("saloon"), Some(&"calm".to_string()));
    assert_eq!(
        config.mood_aliases.get("riding"),
        Some(&"exploration".to_string())
    );
}

#[test]
fn audio_config_mood_aliases_defaults_empty() {
    let yaml = r#"
mood_tracks: {}
sfx_library: {}
creature_voice_presets: {}
mixer:
  music_volume: 0.8
  sfx_volume: 0.9
  voice_volume: 1.0
  crossfade_default_ms: 500
"#;

    let config: AudioConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        config.mood_aliases.is_empty(),
        "missing mood_aliases should default to empty HashMap"
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: MusicDirector resolves mood string through alias chain
// ═══════════════════════════════════════════════════════════

#[test]
fn resolve_mood_direct_core_mood() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("combat");
    assert_eq!(
        resolved,
        MoodKey::COMBAT,
        "core mood should resolve directly"
    );
}

#[test]
fn resolve_mood_through_alias() {
    let aliases = HashMap::from([("standoff".to_string(), "tension".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("standoff");
    assert_eq!(
        resolved,
        MoodKey::TENSION,
        "standoff should resolve to tension via alias"
    );
}

#[test]
fn resolve_mood_saloon_to_calm() {
    let aliases = HashMap::from([("saloon".to_string(), "calm".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("saloon");
    assert_eq!(resolved, MoodKey::CALM);
}

#[test]
fn resolve_mood_riding_to_exploration() {
    let aliases = HashMap::from([("riding".to_string(), "exploration".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("riding");
    assert_eq!(resolved, MoodKey::EXPLORATION);
}

#[test]
fn resolve_mood_convoy_to_exploration() {
    let aliases = HashMap::from([("convoy".to_string(), "exploration".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("convoy");
    assert_eq!(resolved, MoodKey::EXPLORATION);
}

#[test]
fn resolve_mood_cyberspace_to_mystery() {
    let aliases = HashMap::from([("cyberspace".to_string(), "mystery".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("cyberspace");
    assert_eq!(resolved, MoodKey::MYSTERY);
}

#[test]
fn resolve_mood_chained_alias() {
    // Two-level alias: "showdown" → "standoff" → "tension"
    let aliases = HashMap::from([
        ("showdown".to_string(), "standoff".to_string()),
        ("standoff".to_string(), "tension".to_string()),
    ]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("showdown");
    assert_eq!(
        resolved,
        MoodKey::TENSION,
        "chained aliases should resolve transitively"
    );
}

#[test]
fn resolve_mood_case_insensitive() {
    let aliases = HashMap::from([("standoff".to_string(), "tension".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("Standoff");
    assert_eq!(
        resolved,
        MoodKey::TENSION,
        "resolution should be case-insensitive"
    );
}

// ═══════════════════════════════════════════════════════════
// AC5: Confrontation types declare their mood string
// ═══════════════════════════════════════════════════════════

#[test]
fn encounter_mood_override_resolves_through_aliases() {
    let aliases = HashMap::from([("standoff".to_string(), "tension".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.encounter_mood_override = Some("standoff".to_string());

    let classification = director.classify_mood("", &ctx);
    assert_eq!(
        classification.primary,
        MoodKey::TENSION,
        "encounter mood override should resolve through alias chain"
    );
}

#[test]
fn encounter_mood_override_direct_core_mood() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let mut ctx = default_mood_context();
    ctx.encounter_mood_override = Some("combat".to_string());

    let classification = director.classify_mood("", &ctx);
    assert_eq!(classification.primary, MoodKey::COMBAT);
}

// ═══════════════════════════════════════════════════════════
// AC6: Unknown moods fall back through alias chain or to default
// ═══════════════════════════════════════════════════════════

#[test]
fn unknown_mood_falls_back_to_exploration() {
    let config = audio_config_with_aliases(HashMap::new());
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("completely_unknown_mood");
    assert_eq!(
        resolved,
        MoodKey::EXPLORATION,
        "unknown mood with no alias should fall back to exploration"
    );
}

#[test]
fn unknown_mood_with_custom_tracks_uses_key_directly() {
    let mut config = audio_config_with_aliases(HashMap::new());
    // Add tracks for a custom mood key (no alias needed — tracks exist directly)
    config.mood_tracks.insert(
        "standoff".to_string(),
        vec![sidequest_genre::MoodTrack {
            path: "audio/music/standoff_full.ogg".to_string(),
            title: "Showdown".to_string(),
            bpm: 50,
            energy: 0.7,
        }],
    );
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("standoff");
    assert_eq!(
        resolved.as_str(),
        "standoff",
        "mood with direct tracks should resolve to itself, not fall back"
    );
}

#[test]
fn circular_alias_does_not_infinite_loop() {
    // Protect against misconfigured circular aliases
    let aliases = HashMap::from([
        ("a".to_string(), "b".to_string()),
        ("b".to_string(), "a".to_string()),
    ]);
    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    // Should terminate (fall back) rather than infinite loop
    let resolved = director.resolve_mood("a");
    // We don't assert the exact result — just that it terminates and returns something
    let _key = resolved.as_str(); // should not panic or hang
}

#[test]
fn deeply_chained_alias_terminates() {
    // 10-level chain: mood_0 → mood_1 → ... → mood_9 → calm
    let mut aliases = HashMap::new();
    for i in 0..9 {
        aliases.insert(format!("mood_{i}"), format!("mood_{}", i + 1));
    }
    aliases.insert("mood_9".to_string(), "calm".to_string());

    let config = audio_config_with_aliases(aliases);
    let director = MusicDirector::new(&config);

    let resolved = director.resolve_mood("mood_0");
    assert_eq!(
        resolved,
        MoodKey::CALM,
        "deeply chained aliases should eventually resolve"
    );
}

// ═══════════════════════════════════════════════════════════
// AC7: Backward-compatible enum→string mapping
// ═══════════════════════════════════════════════════════════

#[test]
fn mood_key_as_key_matches_old_enum_keys() {
    // The old Mood enum's as_key() returned these exact strings.
    // MoodKey constants must match for backward compatibility.
    assert_eq!(MoodKey::COMBAT.as_str(), "combat");
    assert_eq!(MoodKey::EXPLORATION.as_str(), "exploration");
    assert_eq!(MoodKey::TENSION.as_str(), "tension");
    assert_eq!(MoodKey::TRIUMPH.as_str(), "triumph");
    assert_eq!(MoodKey::SORROW.as_str(), "sorrow");
    assert_eq!(MoodKey::MYSTERY.as_str(), "mystery");
    assert_eq!(MoodKey::CALM.as_str(), "calm");
}

#[test]
fn mood_key_is_core_mood() {
    assert!(MoodKey::COMBAT.is_core());
    assert!(MoodKey::EXPLORATION.is_core());
    assert!(MoodKey::TENSION.is_core());
    assert!(MoodKey::TRIUMPH.is_core());
    assert!(MoodKey::SORROW.is_core());
    assert!(MoodKey::MYSTERY.is_core());
    assert!(MoodKey::CALM.is_core());
}

#[test]
fn custom_mood_is_not_core() {
    let custom = MoodKey::from("standoff");
    assert!(
        !custom.is_core(),
        "standoff is a custom mood, not a core mood"
    );
}

#[test]
fn mood_key_from_old_json_deserializes() {
    // Old saves might have mood as a string like "Combat" (capitalized enum variant)
    let json = r#""Combat""#;
    let mood: MoodKey = serde_json::from_str(json).unwrap();
    assert_eq!(
        mood,
        MoodKey::COMBAT,
        "capitalized enum variant should deserialize to lowercase core mood"
    );
}

#[test]
fn mood_key_from_lowercase_json_deserializes() {
    let json = r#""tension""#;
    let mood: MoodKey = serde_json::from_str(json).unwrap();
    assert_eq!(mood, MoodKey::TENSION);
}

#[test]
fn music_director_select_track_for_custom_mood() {
    let mut config = audio_config_with_aliases(HashMap::new());
    config.mood_tracks.insert(
        "standoff".to_string(),
        vec![sidequest_genre::MoodTrack {
            path: "audio/music/standoff_full.ogg".to_string(),
            title: "The Ecstasy of Gold".to_string(),
            bpm: 50,
            energy: 0.7,
        }],
    );
    let mut director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::from("standoff"),
        intensity: 0.8,
        confidence: 0.9,
    };
    let ctx = default_mood_context();

    // Director should find tracks for the custom mood key
    let result = director.evaluate_narration_with_classification(&classification, &ctx);
    // Should produce a cue because standoff tracks exist
    assert!(
        matches!(
            result,
            sidequest_game::music_director::MusicEvalResult::Cue(_)
        ),
        "custom mood with tracks should produce a music cue, got: {result:?}"
    );
}

#[test]
fn music_director_falls_back_to_alias_tracks() {
    let aliases = HashMap::from([("standoff".to_string(), "tension".to_string())]);
    let config = audio_config_with_aliases(aliases);
    let mut director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::from("standoff"),
        intensity: 0.8,
        confidence: 0.9,
    };
    let ctx = default_mood_context();

    // standoff has no direct tracks, but resolves to tension which does
    let result = director.evaluate_narration_with_classification(&classification, &ctx);
    assert!(
        matches!(
            result,
            sidequest_game::music_director::MusicEvalResult::Cue(_)
        ),
        "aliased mood should fall back to alias target's tracks, got: {result:?}"
    );
}
