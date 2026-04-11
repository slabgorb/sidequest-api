//! Story 35-14: OTEL watcher event for unknown AudioVariation type.
//!
//! `sidequest_genre::models::audio::AudioVariation::as_variation()` silently
//! defaults unrecognized `variation_type` strings to `TrackVariation::Full`
//! with only a `tracing::warn!` — no watcher event reaches the GM panel.
//! A genre pack author who writes `variation_type: "typo_here"` will see the
//! track register as `Full` in `themed_tracks`, and if the subsequent
//! `select_variation` call happens to prefer Full, Path A fires and returns
//! Full with no emission. The player hears the wrong track (labeled
//! incorrectly), the GM panel sees nothing.
//!
//! Story 35-13 Pass 3 Reviewer flagged this as a pre-existing out-of-scope
//! gap. Keith's direction: "we fix that fallback immediately". Story 35-14
//! closes it by detecting the unknown `variation_type` at the sole
//! production call site (`MusicDirector::new()` at music_director.rs:369)
//! and emitting a `sidequest_genre.unknown_variation_type` watcher event
//! before the `as_variation()` fallback fires.
//!
//! sidequest-genre itself does not depend on sidequest-telemetry, so the
//! detection lives in the caller (sidequest-game::music_director) where the
//! WatcherEventBuilder is already in scope. This keeps the crate boundary
//! clean and matches the 35-13 pattern.

use std::collections::HashMap;

use sidequest_game::MusicDirector;
use sidequest_genre::{AudioConfig, AudioTheme, AudioVariation, MixerConfig};
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ---------------------------------------------------------------------------
// Test infrastructure — matches otel_chargen_subsystems_story_35_13_tests.rs.
// ---------------------------------------------------------------------------

static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("channel must be initialized");
    while rx.try_recv().is_ok() {}
    (guard, rx)
}

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

fn find_events(events: &[WatcherEvent], component: &str, action: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == component
                && e.fields.get("action").and_then(serde_json::Value::as_str) == Some(action)
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Minimal AudioConfig with a single AudioTheme whose variations vec contains
/// one AudioVariation with an unrecognized `variation_type` string.
/// `as_variation()` will default this to `TrackVariation::Full` with only a
/// `tracing::warn!` — before story 35-14, the GM panel never learned about
/// the malformed content.
fn audio_config_with_unknown_variation_type() -> AudioConfig {
    AudioConfig {
        mood_tracks: HashMap::new(),
        mood_keywords: HashMap::new(),
        sfx_library: HashMap::new(),
        creature_voice_presets: HashMap::new(),
        mixer: MixerConfig {
            music_volume: 0.6,
            sfx_volume: 0.8,
            voice_volume: 1.0,
            crossfade_default_ms: 3000,
        },
        themes: vec![AudioTheme {
            name: "battle_typo".to_string(),
            mood: "combat".to_string(),
            base_prompt: "epic battle".to_string(),
            variations: vec![AudioVariation {
                // Deliberate typo — should be "full" but author wrote "fill"
                variation_type: "fill".to_string(),
                path: "audio/themes/combat/fill_typo.ogg".to_string(),
            }],
        }],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

/// AudioConfig with a mix of known and unknown variation types. The unknown
/// one should produce a watcher event; the known ones should not.
fn audio_config_with_mixed_variation_types() -> AudioConfig {
    AudioConfig {
        mood_tracks: HashMap::new(),
        mood_keywords: HashMap::new(),
        sfx_library: HashMap::new(),
        creature_voice_presets: HashMap::new(),
        mixer: MixerConfig {
            music_volume: 0.6,
            sfx_volume: 0.8,
            voice_volume: 1.0,
            crossfade_default_ms: 3000,
        },
        themes: vec![AudioTheme {
            name: "forest".to_string(),
            mood: "exploration".to_string(),
            base_prompt: "ambient forest".to_string(),
            variations: vec![
                AudioVariation {
                    variation_type: "full".to_string(),
                    path: "audio/themes/exploration/full.ogg".to_string(),
                },
                AudioVariation {
                    variation_type: "ambient".to_string(),
                    path: "audio/themes/exploration/ambient.ogg".to_string(),
                },
                AudioVariation {
                    // Typo — should be "tension_build"
                    variation_type: "tenzion_build".to_string(),
                    path: "audio/themes/exploration/tenzion_build.ogg".to_string(),
                },
            ],
        }],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn unknown_variation_type_emits_watcher_event_on_director_construction() {
    let (_guard, mut rx) = fresh_subscriber();

    let config = audio_config_with_unknown_variation_type();
    // Simply constructing the MusicDirector should detect the unknown
    // variation_type and emit a watcher event — BEFORE any select_variation
    // call. The detection lives at the construction-time iteration point
    // (music_director.rs around line 369 where variation.as_variation() is
    // called) so malformed content surfaces at startup, not at first use.
    let _director = MusicDirector::new(&config);

    let events = drain_events(&mut rx);
    let unknown = find_events(&events, "music_director", "unknown_variation_type");

    assert!(
        !unknown.is_empty(),
        "MusicDirector::new() must emit music_director.unknown_variation_type \
         watcher event when an AudioTheme variation has an unrecognized \
         variation_type string. Currently sidequest-genre::AudioVariation::\
         as_variation() defaults silently to TrackVariation::Full with only \
         a tracing::warn!, so the GM panel cannot detect the malformed \
         content. Got {} other events.",
        events.len()
    );

    let evt = &unknown[0];
    assert_eq!(
        evt.fields
            .get("variation_type")
            .and_then(serde_json::Value::as_str),
        Some("fill"),
        "unknown_variation_type event must record the unrecognized string \
         so the GM panel can surface the exact typo"
    );
    assert_eq!(
        evt.fields.get("theme").and_then(serde_json::Value::as_str),
        Some("battle_typo"),
        "unknown_variation_type event must record the theme name for diagnosis"
    );
    assert_eq!(
        evt.fields.get("mood").and_then(serde_json::Value::as_str),
        Some("combat"),
        "unknown_variation_type event must record the mood key"
    );
    assert_eq!(
        evt.fields
            .get("fallback_to")
            .and_then(serde_json::Value::as_str),
        Some("full"),
        "unknown_variation_type event must record what the fallback resolved to"
    );
}

#[test]
fn known_variation_types_do_not_emit_unknown_variation_event() {
    let (_guard, mut rx) = fresh_subscriber();

    // Config with three variations — two known ("full", "ambient") and one
    // unknown ("tenzion_build"). Only the unknown one should emit an event.
    let config = audio_config_with_mixed_variation_types();
    let _director = MusicDirector::new(&config);

    let events = drain_events(&mut rx);
    let unknown = find_events(&events, "music_director", "unknown_variation_type");

    assert_eq!(
        unknown.len(),
        1,
        "MusicDirector::new() must emit EXACTLY ONE \
         music_director.unknown_variation_type event when the config has \
         one unknown variation_type among known ones. Known types must not \
         trigger the event. Got {} unknown_variation_type events.",
        unknown.len()
    );

    let evt = &unknown[0];
    assert_eq!(
        evt.fields
            .get("variation_type")
            .and_then(serde_json::Value::as_str),
        Some("tenzion_build"),
        "the one emitted event must correspond to the unknown variation_type, \
         not one of the known ones"
    );
}

// ===========================================================================
// A5 wiring assertion — the detection must live at the production call site.
// ===========================================================================

#[test]
fn wiring_music_director_new_calls_as_variation() {
    // The unknown_variation_type detection must live at the same call site
    // that invokes `variation.as_variation()`. If a future refactor moves
    // the call elsewhere without moving the detection, malformed content
    // silently slips back through. This grep guards the wiring.
    let src = include_str!("../src/music_director.rs");
    assert!(
        src.contains("variation.as_variation()") || src.contains(".as_variation()"),
        "music_director.rs must still call variation.as_variation() — if \
         this call moves to another location, the unknown_variation_type \
         watcher event may lose its detection site."
    );
    assert!(
        src.contains("unknown_variation_type"),
        "music_director.rs must still contain the string \
         'unknown_variation_type' — if this is removed, the story 35-14 \
         OTEL event is no longer being emitted."
    );
}
