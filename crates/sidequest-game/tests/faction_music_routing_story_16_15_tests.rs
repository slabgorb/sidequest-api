//! Story 16-15: Faction music routing — trigger faction themes by context
//!
//! RED phase tests. Verify MusicDirector checks location faction, confrontation
//! actor factions, and player reputation to select faction-specific tracks.
//! Road warrior's 10 faction themes as the test case.
//!
//! ACs tested:
//!   AC1: audio.yaml declares faction_themes section with trigger conditions
//!   AC2: MusicDirector checks location faction for faction theme selection
//!   AC3: MusicDirector checks active confrontation actors' factions
//!   AC4: MusicDirector checks player reputation threshold for faction music
//!   AC5: Road warrior faction themes (Bosozoku through Dekotora) as test case
//!   AC6: Faction music overrides default mood-based selection when conditions match
//!   AC7: Falls back to mood-based selection when no faction conditions match

use sidequest_game::music_director::{FactionContext, MoodContext, MusicDirector, MusicEvalResult};
use sidequest_genre::AudioConfig;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn genre_pack_path(genre: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // sidequest-api/
        .unwrap()
        .parent() // oq-1/
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs")
        .join(genre)
}

fn load_audio_yaml(genre: &str) -> AudioConfig {
    let path = genre_pack_path(genre).join("audio.yaml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    serde_yaml::from_str::<AudioConfig>(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

fn road_warrior_director() -> MusicDirector {
    let audio = load_audio_yaml("road_warrior");
    MusicDirector::new(&audio)
}

fn base_mood_ctx() -> MoodContext {
    MoodContext::default()
}

/// Build a FactionContext for location-based faction music.
fn location_faction_ctx(faction_id: &str) -> FactionContext {
    FactionContext {
        location_faction: Some(faction_id.to_string()),
        actor_factions: vec![],
        player_reputation: None,
    }
}

/// Build a FactionContext for actor-based faction music.
fn actor_faction_ctx(factions: &[&str]) -> FactionContext {
    FactionContext {
        location_faction: None,
        actor_factions: factions.iter().map(|s| s.to_string()).collect(),
        player_reputation: None,
    }
}

/// Build a FactionContext with reputation threshold.
fn reputation_faction_ctx(faction_id: &str, reputation: i32) -> FactionContext {
    FactionContext {
        location_faction: None,
        actor_factions: vec![],
        player_reputation: Some((faction_id.to_string(), reputation)),
    }
}

// ═══════════════════════════════════════════════════════════
// AC1: audio.yaml declares faction_themes with trigger conditions
// ═══════════════════════════════════════════════════════════

#[test]
fn road_warrior_audio_has_faction_themes() {
    let audio = load_audio_yaml("road_warrior");
    assert!(
        !audio.faction_themes.is_empty(),
        "road_warrior audio.yaml should declare faction_themes"
    );
}

#[test]
fn faction_theme_has_faction_id() {
    let audio = load_audio_yaml("road_warrior");
    for theme in &audio.faction_themes {
        assert!(
            !theme.faction_id.is_empty(),
            "faction theme must have a faction_id"
        );
    }
}

#[test]
fn faction_theme_has_track_path() {
    let audio = load_audio_yaml("road_warrior");
    for theme in &audio.faction_themes {
        assert!(
            !theme.track.path.is_empty(),
            "faction theme must have a track path"
        );
    }
}

#[test]
fn faction_theme_has_trigger_conditions() {
    let audio = load_audio_yaml("road_warrior");
    for theme in &audio.faction_themes {
        // Each faction theme should have at least one trigger condition
        let has_trigger = theme.triggers.location
            || theme.triggers.npc_present
            || theme.triggers.reputation_threshold.is_some();
        assert!(
            has_trigger,
            "faction theme '{}' must have at least one trigger condition",
            theme.faction_id
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC2: MusicDirector checks location faction
// ═══════════════════════════════════════════════════════════

#[test]
fn location_faction_selects_faction_track() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let faction_ctx = location_faction_ctx("bosozoku");
    let result = director.evaluate_with_faction(
        "The garage district hums with engine noise.",
        &mood_ctx,
        &faction_ctx,
    );
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("bosozoku"),
                "should select bosozoku faction track, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

#[test]
fn different_location_faction_selects_different_track() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let faction_ctx = location_faction_ctx("dekotora");
    let result =
        director.evaluate_with_faction("The truck stop glows with neon.", &mood_ctx, &faction_ctx);
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("dekotora"),
                "should select dekotora faction track, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════
// AC3: MusicDirector checks confrontation actors' factions
// ═══════════════════════════════════════════════════════════

#[test]
fn actor_faction_selects_faction_track() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let faction_ctx = actor_faction_ctx(&["one_percenters"]);
    let result =
        director.evaluate_with_faction("The bikers circle your rig.", &mood_ctx, &faction_ctx);
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id
                    .as_deref()
                    .unwrap_or("")
                    .contains("one_percenters"),
                "should select one_percenters faction track, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

#[test]
fn first_actor_faction_wins_when_multiple() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    // Multiple actor factions — first match should win
    let faction_ctx = actor_faction_ctx(&["cafe_racers", "rockers"]);
    let result = director.evaluate_with_faction(
        "Mixed gang encounter on the highway.",
        &mood_ctx,
        &faction_ctx,
    );
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("cafe_racer"),
                "first faction should win, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════
// AC4: MusicDirector checks player reputation threshold
// ═══════════════════════════════════════════════════════════

#[test]
fn high_reputation_triggers_faction_theme() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    // High reputation with lowriders should trigger their theme
    let faction_ctx = reputation_faction_ctx("lowriders", 8);
    let result = director.evaluate_with_faction(
        "You roll through lowrider territory.",
        &mood_ctx,
        &faction_ctx,
    );
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("lowrider"),
                "high reputation should trigger lowrider theme, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

#[test]
fn low_reputation_does_not_trigger_faction_theme() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    // Low reputation should NOT trigger faction theme
    let faction_ctx = reputation_faction_ctx("lowriders", 1);
    let result = director.evaluate_with_faction(
        "You roll through lowrider territory.",
        &mood_ctx,
        &faction_ctx,
    );
    // Suppressed or NoTrackFound is fine — we only care that the Cue path
    // doesn't contain the faction track.
    if let MusicEvalResult::Cue(cue) = &result {
        assert!(
            !cue.track_id.as_deref().unwrap_or("").contains("lowrider"),
            "low reputation should not trigger faction theme, got: {:?}",
            cue.track_id
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC5: Road warrior 10 faction themes (Bosozoku through Dekotora)
// ═══════════════════════════════════════════════════════════

const ROAD_WARRIOR_FACTIONS: &[(&str, &str)] = &[
    ("bosozoku", "bosozoku"),
    ("cafe_racers", "cafe_racer"),
    ("dekotora", "dekotora"),
    ("lowriders", "lowrider"),
    ("matatu", "matatu"),
    ("mods", "mods"),
    ("one_percenters", "one_percenter"),
    ("raggare", "raggare"),
    ("rockers", "rocker"),
    ("tuk_tuk", "tuk_tuk"),
];

#[test]
fn road_warrior_has_ten_faction_themes() {
    let audio = load_audio_yaml("road_warrior");
    assert!(
        audio.faction_themes.len() >= 10,
        "road_warrior should have at least 10 faction themes, got {}",
        audio.faction_themes.len()
    );
}

#[test]
fn all_road_warrior_factions_have_themes() {
    let audio = load_audio_yaml("road_warrior");
    let faction_ids: Vec<&str> = audio
        .faction_themes
        .iter()
        .map(|t| t.faction_id.as_str())
        .collect();
    for (faction_id, _) in ROAD_WARRIOR_FACTIONS {
        assert!(
            faction_ids.contains(faction_id),
            "missing faction theme for '{}'",
            faction_id
        );
    }
}

#[test]
fn each_faction_theme_has_unique_track() {
    let audio = load_audio_yaml("road_warrior");
    let paths: Vec<&str> = audio
        .faction_themes
        .iter()
        .map(|t| t.track.path.as_str())
        .collect();
    let unique_count = {
        let mut s = std::collections::HashSet::new();
        paths.iter().for_each(|p| {
            s.insert(*p);
        });
        s.len()
    };
    assert_eq!(
        unique_count,
        paths.len(),
        "each faction should have a unique track path"
    );
}

// ═══════════════════════════════════════════════════════════
// AC6: Faction music overrides mood-based selection
// ═══════════════════════════════════════════════════════════

#[test]
fn faction_overrides_mood_based_selection_in_combat() {
    let mut director = road_warrior_director();
    let mut mood_ctx = base_mood_ctx();
    mood_ctx.in_combat = true; // Combat would normally select combat mood
    let faction_ctx = location_faction_ctx("mods");
    let result =
        director.evaluate_with_faction("The mods are everywhere.", &mood_ctx, &faction_ctx);
    match &result {
        MusicEvalResult::Cue(cue) => {
            // Faction theme should override combat mood
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("mods"),
                "faction should override combat mood, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

#[test]
fn faction_overrides_mood_based_selection_in_exploration() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx(); // Default = exploration-ish
    let faction_ctx = location_faction_ctx("raggare");
    let result =
        director.evaluate_with_faction("The open road stretches ahead.", &mood_ctx, &faction_ctx);
    match &result {
        MusicEvalResult::Cue(cue) => {
            assert!(
                cue.track_id.as_deref().unwrap_or("").contains("raggare"),
                "faction should override exploration mood, got: {:?}",
                cue.track_id
            );
        }
        other => panic!("expected Cue with faction track, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════
// AC7: Falls back to mood-based when no faction conditions match
// ═══════════════════════════════════════════════════════════

#[test]
fn no_faction_context_falls_back_to_mood() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let empty_faction = FactionContext::default();
    let result =
        director.evaluate_with_faction("The desert wind howls.", &mood_ctx, &empty_faction);
    // Suppressed or NoTrackFound is fine for fallback — only assert on the Cue path.
    if let MusicEvalResult::Cue(cue) = &result {
        // Should NOT contain any faction track
        let track = cue.track_id.as_deref().unwrap_or("");
        assert!(
            !track.contains("faction_"),
            "no faction context should fall back to mood-based, got: {:?}",
            track
        );
    }
}

#[test]
fn unknown_faction_falls_back_to_mood() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let faction_ctx = location_faction_ctx("nonexistent_faction");
    let result = director.evaluate_with_faction("Somewhere unknown.", &mood_ctx, &faction_ctx);
    // Suppressed or NoTrackFound is acceptable fallback — only check Cue path.
    if let MusicEvalResult::Cue(cue) = &result {
        let track = cue.track_id.as_deref().unwrap_or("");
        assert!(
            !track.contains("nonexistent"),
            "unknown faction should fall back to mood-based, got: {:?}",
            track
        );
    }
}

#[test]
fn no_faction_themes_in_genre_falls_back_to_mood() {
    // Load a genre that doesn't have faction themes (e.g., neon_dystopia)
    let audio = load_audio_yaml("neon_dystopia");
    let mut director = MusicDirector::new(&audio);
    let mood_ctx = base_mood_ctx();
    let faction_ctx = location_faction_ctx("some_faction");
    let result = director.evaluate_with_faction("The neon streets glow.", &mood_ctx, &faction_ctx);
    // Should not panic — graceful fallback. Suppressed or NoTrackFound is fine.
    if let MusicEvalResult::Cue(cue) = &result {
        let track = cue.track_id.as_deref().unwrap_or("");
        assert!(
            !track.contains("faction_"),
            "genre without faction themes should use mood-based, got: {:?}",
            track
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Integration: evaluate_with_faction exists and is callable
// ═══════════════════════════════════════════════════════════

#[test]
fn evaluate_with_faction_method_exists() {
    let mut director = road_warrior_director();
    let mood_ctx = base_mood_ctx();
    let faction_ctx = FactionContext::default();
    // Just verify the method compiles and returns something
    let _result = director.evaluate_with_faction("Test narration.", &mood_ctx, &faction_ctx);
}

#[test]
fn faction_context_default_has_no_faction() {
    let ctx = FactionContext::default();
    assert!(ctx.location_faction.is_none());
    assert!(ctx.actor_factions.is_empty());
    assert!(ctx.player_reputation.is_none());
}
