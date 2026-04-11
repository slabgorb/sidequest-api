//! Story 35-13: OTEL watcher events for chargen subsystems + AudioVariation fallback.
//!
//! Verifies that four subsystems — `chargen.stats`, `chargen.backstory`,
//! `chargen.hp_formula`, and the `music_director.variation_fallback` path —
//! emit `WatcherEvent`s so the GM panel can observe character generation
//! and audio variation selection in real time (ADR-031 / CLAUDE.md OTEL rule).
//!
//! Three of these subsystems currently emit only `tracing::info_span!`
//! events, which reach stdout but NOT the `sidequest-telemetry` broadcast
//! channel that the GM panel watches. Without `WatcherEventBuilder` calls,
//! the GM panel cannot distinguish actual subsystem engagement from Claude
//! improvising chargen values.
//!
//! The AudioVariation fallback in `MusicDirector::select_variation()` is a
//! silent degradation path — when the preferred variation isn't available
//! for the current mood, the director falls back to `Full` (or any), but
//! emits only `tracing::warn!`. CLAUDE.md's "no silent fallbacks" rule
//! requires the fallback be surfaced on the watcher channel.
//!
//! Follows the 35-8 / 35-9 pattern (beat_filter, scene_relevance, NPC
//! subsystems) using `WatcherEventBuilder` from `sidequest-telemetry`.
//!
//! Each subsystem is exercised directly, and a wiring assertion grep confirms
//! the production code path that reaches each subsystem is still in place
//! (CLAUDE.md A5 — "Every Test Suite Needs a Wiring Test").

use std::collections::HashMap;

use sidequest_game::builder::CharacterBuilder;
use sidequest_game::{MoodClassification, MoodContext, MoodKey, MusicDirector, TrackVariation};
use sidequest_genre::{
    AudioConfig, AudioTheme, AudioVariation, BackstoryTables, CharCreationChoice,
    CharCreationScene, MechanicalEffects, MixerConfig, MoodTrack, RulesConfig,
};
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ---------------------------------------------------------------------------
// Test infrastructure — matches otel_npc_subsystems_story_35_9_tests.rs.
// ---------------------------------------------------------------------------

/// Serialize telemetry tests — the global broadcast channel is shared state,
/// so tests that emit and read events must not run concurrently.
static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Initialize the global telemetry channel (idempotent via OnceLock),
/// acquire the serialization lock, drain any stale events, and return a
/// clean receiver.
fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
    // Recover from a previously-poisoned lock: a panic in an earlier test
    // should fail that test only, not cascade into every subsequent test.
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("channel must be initialized");
    while rx.try_recv().is_ok() {}
    (guard, rx)
}

/// Drain every currently-buffered event from the receiver.
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// Find events emitted by `component` whose `action` field matches `action`.
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
// Chargen fixtures — minimal C&C-style scenes that drive build() without
// requiring any genre pack on disk.
// ---------------------------------------------------------------------------

fn caverns_scenes() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "the_roll".to_string(),
            title: "3d6. In Order.".to_string(),
            narration: "Six bone dice on the table.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: Some(MechanicalEffects {
                stat_generation: Some("roll_3d6_strict".to_string()),
                ..MechanicalEffects::default()
            }),
        },
        CharCreationScene {
            id: "pronouns".to_string(),
            title: "Who Are You?".to_string(),
            narration: "For the tally.".to_string(),
            choices: vec![CharCreationChoice {
                label: "he/him".to_string(),
                description: "He.".to_string(),
                mechanical_effects: MechanicalEffects {
                    pronoun_hint: Some("he/him".to_string()),
                    ..MechanicalEffects::default()
                },
            }],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
    ]
}

fn rules_3d6() -> RulesConfig {
    RulesConfig {
        tone: "gritty".to_string(),
        lethality: "high".to_string(),
        magic_level: "none".to_string(),
        stat_generation: "roll_3d6_strict".to_string(),
        point_buy_budget: 0,
        ability_score_names: vec![
            "STR".to_string(),
            "DEX".to_string(),
            "CON".to_string(),
            "INT".to_string(),
            "WIS".to_string(),
            "CHA".to_string(),
        ],
        allowed_classes: vec!["Delver".to_string()],
        allowed_races: vec!["Human".to_string()],
        class_hp_bases: HashMap::from([("Delver".to_string(), 8)]),
        default_class: Some("Delver".to_string()),
        default_race: Some("Human".to_string()),
        default_hp: Some(8),
        default_ac: Some(10),
        default_location: Some("The mouth of the dungeon".to_string()),
        default_time_of_day: Some("dawn".to_string()),
        hp_formula: Some("8 + CON_modifier".to_string()),
        banned_spells: vec![],
        custom_rules: HashMap::new(),
        stat_display_fields: vec![],
        encounter_base_tension: HashMap::new(),
        race_label: None,
        class_label: None,
        confrontations: vec![],
        resources: vec![],
        xp_affinity: None,
        initiative_rules: HashMap::new(),
    }
}

fn rules_without_hp_formula() -> RulesConfig {
    RulesConfig {
        hp_formula: None,
        ..rules_3d6()
    }
}

fn test_backstory_tables() -> BackstoryTables {
    BackstoryTables {
        template: "Former {trade}. {feature}.".to_string(),
        tables: HashMap::from([
            (
                "trade".to_string(),
                vec!["ratcatcher".to_string(), "gravedigger".to_string()],
            ),
            (
                "feature".to_string(),
                vec![
                    "Missing three fingers".to_string(),
                    "Walks with a limp".to_string(),
                ],
            ),
        ]),
    }
}

/// Drive a builder through the minimal C&C scenes and build a character.
/// Returns `()` — tests that care about the character inspect it separately;
/// this helper only exists to exercise the chargen pipeline end-to-end so
/// the watcher events get emitted.
fn build_test_character(rules: &RulesConfig, tables: Option<BackstoryTables>) {
    let scenes = caverns_scenes();
    let mut builder = CharacterBuilder::new(scenes, rules, tables);
    builder
        .apply_freeform("")
        .expect("the_roll scene auto-advances on empty input");
    builder
        .apply_choice(0)
        .expect("pronouns scene has one choice");
    let _ = builder
        .build("Grist the Ratcatcher")
        .expect("build should succeed");
}

// ===========================================================================
// AC-1 — chargen.stats: stats generation emits WatcherEvent
// ===========================================================================

#[test]
fn chargen_stats_generation_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_3d6(), None);

    let events = drain_events(&mut rx);
    let stats = find_events(&events, "chargen", "stats_generated");

    assert!(
        !stats.is_empty(),
        "CharacterBuilder::build() must emit chargen.stats_generated watcher event \
         when stats are rolled/allocated; got {} other events from chargen subsystem. \
         Currently only a tracing::info_span! is emitted, which does not reach the \
         GM panel broadcast channel.",
        events.iter().filter(|e| e.component == "chargen").count()
    );

    let evt = &stats[0];
    assert_eq!(
        evt.fields.get("method").and_then(serde_json::Value::as_str),
        Some("roll_3d6_strict"),
        "stats_generated must record the generation method"
    );
    assert!(
        evt.fields
            .get("stat_count")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n == 6)
            .unwrap_or(false),
        "stats_generated must record stat_count=6 for the C&C rules fixture, \
         got {:?}",
        evt.fields.get("stat_count")
    );
}

#[test]
fn chargen_stats_generation_records_stat_values() {
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_3d6(), None);

    let events = drain_events(&mut rx);
    let stats = find_events(&events, "chargen", "stats_generated");
    assert!(!stats.is_empty(), "chargen.stats_generated must be emitted");

    let evt = &stats[0];
    // The event should carry at least one rolled stat value so the GM panel
    // can sanity-check the roll distribution. We don't mandate a specific
    // serialization shape, just that CON appears somewhere in the fields.
    let has_con = evt.fields.iter().any(|(k, v)| {
        k.contains("CON")
            || v.as_str().map(|s| s.contains("CON")).unwrap_or(false)
            || v.as_object()
                .map(|obj| obj.contains_key("CON"))
                .unwrap_or(false)
    });
    assert!(
        has_con,
        "chargen.stats_generated must include rolled stat values \
         (expected CON to appear in event fields); got fields: {:?}",
        evt.fields.keys().collect::<Vec<_>>()
    );
}

// ===========================================================================
// AC-2 — chargen.hp_formula: HP formula evaluation emits WatcherEvent
// ===========================================================================

#[test]
fn chargen_hp_formula_evaluation_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_3d6(), None);

    let events = drain_events(&mut rx);
    let hp_events = find_events(&events, "chargen", "hp_formula_evaluated");

    assert!(
        !hp_events.is_empty(),
        "CharacterBuilder::build() must emit chargen.hp_formula_evaluated \
         watcher event when hp_formula is set. Currently only a \
         tracing::info_span! is emitted, which does not reach the GM panel."
    );

    let evt = &hp_events[0];
    assert_eq!(
        evt.fields
            .get("formula")
            .and_then(serde_json::Value::as_str),
        Some("8 + CON_modifier"),
        "hp_formula_evaluated must record the formula string"
    );
    assert_eq!(
        evt.fields.get("class").and_then(serde_json::Value::as_str),
        Some("Delver"),
        "hp_formula_evaluated must record the class being evaluated"
    );
    assert!(
        evt.fields
            .get("hp_result")
            .and_then(serde_json::Value::as_i64)
            .is_some(),
        "hp_formula_evaluated must record the numeric hp_result"
    );
    assert!(
        evt.fields.contains_key("con_modifier"),
        "hp_formula_evaluated must record the CON modifier used, \
         so the GM panel can verify the formula wiring end-to-end"
    );
}

#[test]
fn chargen_hp_formula_fallback_emits_watcher_event_when_no_formula() {
    // When no hp_formula is set, the builder falls back to class_hp_bases
    // lookup. That's still a chargen HP decision the GM panel needs to see
    // — emit an event recording the fallback path.
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_without_hp_formula(), None);

    let events = drain_events(&mut rx);
    let hp_events: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "chargen"
                && e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .map(|a| a == "hp_formula_evaluated" || a == "hp_fallback")
                    .unwrap_or(false)
        })
        .collect();

    assert!(
        !hp_events.is_empty(),
        "CharacterBuilder::build() must emit a chargen HP watcher event \
         even when hp_formula is None — silent class_hp_bases fallback \
         violates the 'no silent fallbacks' rule."
    );
}

// ===========================================================================
// AC-3 — chargen.backstory: backstory composition emits WatcherEvent
// ===========================================================================

#[test]
fn chargen_backstory_composed_emits_watcher_event_from_tables() {
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_3d6(), Some(test_backstory_tables()));

    let events = drain_events(&mut rx);
    let backstory = find_events(&events, "chargen", "backstory_composed");

    assert!(
        !backstory.is_empty(),
        "CharacterBuilder::build() must emit chargen.backstory_composed \
         watcher event when composing backstory from tables. Currently \
         only a tracing::info_span! is emitted, which does not reach the \
         GM panel."
    );

    let evt = &backstory[0];
    assert_eq!(
        evt.fields.get("method").and_then(serde_json::Value::as_str),
        Some("tables"),
        "backstory_composed must record the composition method; C&C with \
         backstory_tables should record method=tables"
    );
    // The event should carry the composed length so the GM panel can
    // distinguish a substantive backstory from an empty-string fallback.
    assert!(
        evt.fields
            .get("length")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n > 0)
            .unwrap_or(false),
        "backstory_composed must record composed string length > 0, \
         got fields: {:?}",
        evt.fields.keys().collect::<Vec<_>>()
    );
}

#[test]
fn chargen_backstory_composed_emits_watcher_event_from_fallback() {
    // No backstory_fragments accumulated AND no backstory_tables → fallback
    // branch ("A wanderer with a mysterious past"). This silent branch is
    // the most dangerous — the GM panel must be able to see when chargen
    // is using fallback prose because a genre pack is incomplete.
    let (_guard, mut rx) = fresh_subscriber();

    build_test_character(&rules_3d6(), None);

    let events = drain_events(&mut rx);
    let backstory = find_events(&events, "chargen", "backstory_composed");

    assert!(
        !backstory.is_empty(),
        "CharacterBuilder::build() must emit chargen.backstory_composed \
         even on the fallback branch — silent 'wanderer with a mysterious \
         past' selection violates the no-silent-fallbacks rule."
    );

    let evt = &backstory[0];
    assert_eq!(
        evt.fields.get("method").and_then(serde_json::Value::as_str),
        Some("fallback"),
        "backstory_composed must record method=fallback when no fragments \
         or tables are available"
    );
}

// ===========================================================================
// AC-4 — music_director: AudioVariation fallback emits WatcherEvent
// ===========================================================================

fn audio_config_exploration_full_only() -> AudioConfig {
    // Only the `full` variation is registered for exploration — any request
    // for a non-Full variation (e.g., Overture, TensionBuild) must fall back
    // to Full and emit a watcher event.
    let mut mood_tracks = HashMap::new();
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
            crossfade_default_ms: 3000,
        },
        themes: vec![AudioTheme {
            name: "forest".to_string(),
            mood: "exploration".to_string(),
            base_prompt: "ambient forest".to_string(),
            variations: vec![AudioVariation {
                variation_type: "full".to_string(),
                path: "audio/themes/exploration/full.ogg".to_string(),
            }],
        }],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

fn audio_config_exploration_sparse_only() -> AudioConfig {
    // Neither preferred (Overture) nor Full are available — only Sparse.
    // The fallback path should land on "first available" and emit the
    // watcher event with a reason indicating Full was also missing.
    let mut config = audio_config_exploration_full_only();
    config.themes = vec![AudioTheme {
        name: "forest".to_string(),
        mood: "exploration".to_string(),
        base_prompt: "ambient forest".to_string(),
        variations: vec![AudioVariation {
            variation_type: "sparse".to_string(),
            path: "audio/themes/exploration/sparse.ogg".to_string(),
        }],
    }];
    config
}

/// Themes are registered for `exploration` only. A classification with
/// `primary: MoodKey::COMBAT` will find nothing in `themed_tracks` and
/// hit the outer-`if let Some` no-match path in `select_variation()`.
/// Also registers a `combat` mood_track so that the director's
/// construction doesn't panic on the missing mood, but crucially the
/// `themes` vec has no combat entry — so `themed_tracks["combat"]` is
/// `None` when `select_variation()` runs.
fn audio_config_themes_for_exploration_only() -> AudioConfig {
    let mut mood_tracks = HashMap::new();
    mood_tracks.insert(
        "exploration".to_string(),
        vec![MoodTrack {
            path: "audio/music/explore_1.ogg".to_string(),
            title: "Wanderer's Path".to_string(),
            bpm: 90,
            energy: 0.4,
        }],
    );
    mood_tracks.insert(
        "combat".to_string(),
        vec![MoodTrack {
            path: "audio/music/combat_1.ogg".to_string(),
            title: "Battle Drums".to_string(),
            bpm: 140,
            energy: 0.9,
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
            crossfade_default_ms: 3000,
        },
        // Only exploration has themed variations — combat does not.
        themes: vec![AudioTheme {
            name: "forest".to_string(),
            mood: "exploration".to_string(),
            base_prompt: "ambient forest".to_string(),
            variations: vec![AudioVariation {
                variation_type: "full".to_string(),
                path: "audio/themes/exploration/full.ogg".to_string(),
            }],
        }],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

#[test]
fn audio_variation_fallback_to_full_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let config = audio_config_exploration_full_only();
    let director = MusicDirector::new(&config);

    // Request Overture (session_start=true) but only Full is available.
    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = MoodContext {
        session_start: true,
        ..Default::default()
    };

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Full,
        "When only Full is registered, Overture request must fall back to Full"
    );

    let events = drain_events(&mut rx);
    let fallback = find_events(&events, "music_director", "variation_fallback");

    assert!(
        !fallback.is_empty(),
        "select_variation() must emit music_director.variation_fallback \
         watcher event when falling back from preferred to Full. Currently \
         only a tracing::warn! is emitted, which does not reach the GM panel \
         and violates CLAUDE.md's no-silent-fallbacks rule."
    );

    let evt = &fallback[0];
    assert_eq!(
        evt.fields.get("mood").and_then(serde_json::Value::as_str),
        Some("exploration"),
        "variation_fallback must record the mood key"
    );
    assert_eq!(
        evt.fields
            .get("preferred")
            .and_then(serde_json::Value::as_str),
        Some("overture"),
        "variation_fallback must record the preferred variation that was unavailable"
    );
    assert_eq!(
        evt.fields
            .get("selected")
            .and_then(serde_json::Value::as_str),
        Some("full"),
        "variation_fallback must record the variation actually selected"
    );
}

#[test]
fn audio_variation_fallback_to_first_available_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let config = audio_config_exploration_sparse_only();
    let director = MusicDirector::new(&config);

    // Request Overture, but neither Overture NOR Full are registered — only Sparse.
    let classification = MoodClassification {
        primary: MoodKey::EXPLORATION,
        intensity: 0.5,
        confidence: 0.8,
    };
    let ctx = MoodContext {
        session_start: true,
        ..Default::default()
    };

    let variation = director.select_variation(&classification, &ctx);
    assert_eq!(
        variation,
        TrackVariation::Sparse,
        "When only Sparse is registered, Overture request must land on Sparse"
    );

    let events = drain_events(&mut rx);
    let fallback = find_events(&events, "music_director", "variation_fallback");

    assert!(
        !fallback.is_empty(),
        "select_variation() must emit music_director.variation_fallback \
         watcher event when falling back to first-available variation"
    );

    let evt = &fallback[0];
    assert_eq!(
        evt.fields
            .get("selected")
            .and_then(serde_json::Value::as_str),
        Some("sparse"),
        "variation_fallback must record the actually-selected variation"
    );
    // The GM panel needs to know WHY the fallback reached "first available"
    // — specifically, that Full was also missing for this mood.
    assert!(
        evt.fields.contains_key("reason") || evt.fields.contains_key("full_available"),
        "variation_fallback must record why first-available was used \
         (either a `reason` field or a `full_available=false` flag), \
         got fields: {:?}",
        evt.fields.keys().collect::<Vec<_>>()
    );
}

// ===========================================================================
// AC-4 (rework) — the outer `if let Some(mood_variations)` fallback path.
// ===========================================================================
//
// Reviewer finding on story 35-13: when `themed_tracks` has no entry for the
// classified mood key at all (e.g., a genre pack that registers a new mood in
// char_creation.yaml without a matching theme bundle in audio_config.yaml),
// `select_variation()` silently returns `preferred` without emitting any
// watcher event. This is the exact class of silent fallback the story was
// written to surface. AC-4 covers "audio variation generation fails and
// system falls back to default" — the missing-mood-key path IS a fallback
// to default, just at the OUTER level, before the intra-mood branches that
// stories 35-13 Pass 1 already instrumented.

#[test]
fn audio_variation_fallback_when_mood_missing_from_themed_tracks() {
    let (_guard, mut rx) = fresh_subscriber();

    // Themes registered for EXPLORATION only. Combat mood_tracks exist
    // (so the director has fallback material via mood_tracks), but NO
    // combat theme bundle — `themed_tracks["combat"]` is None.
    let config = audio_config_themes_for_exploration_only();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::COMBAT,
        intensity: 0.8,
        confidence: 0.9,
    };
    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    // Call select_variation — we don't care what it RETURNS (the return
    // value falls through to mood_tracks in evaluate()); we care that
    // the missing-theme condition is surfaced on the watcher channel.
    let _ = director.select_variation(&classification, &ctx);

    let events = drain_events(&mut rx);
    let fallback = find_events(&events, "music_director", "variation_fallback");

    assert!(
        !fallback.is_empty(),
        "select_variation() must emit music_director.variation_fallback \
         watcher event when the classified mood has NO entry in themed_tracks \
         at all. Currently the outer `if let Some(mood_variations)` has no \
         else branch and returns `preferred` silently — violates CLAUDE.md \
         no-silent-fallbacks and the story's AC-4. Got {} other events.",
        events.len()
    );

    let evt = &fallback[0];
    assert_eq!(
        evt.fields.get("mood").and_then(serde_json::Value::as_str),
        Some("combat"),
        "variation_fallback must record the mood key that had no themed entry"
    );
    // The `reason` field must distinguish this outer-level fallback from
    // the two intra-mood fallbacks (preferred_unavailable, only_first_available).
    let reason = evt
        .fields
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .expect("variation_fallback must carry a reason field");
    assert!(
        reason == "mood_not_in_themed_tracks" || reason.contains("mood"),
        "reason must identify the missing-mood case (suggested: \
         'mood_not_in_themed_tracks'), got {:?}",
        reason
    );
    // `full_available` must be false — there's nothing available for this mood.
    assert_eq!(
        evt.fields
            .get("full_available")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "full_available must be false — the entire mood has no themed tracks"
    );
}

// ---------------------------------------------------------------------------
// Fixture + test for the `Some(empty HashMap)` silent path.
// ---------------------------------------------------------------------------
//
// `MusicDirector::new()` uses `themed_tracks.entry(mood).or_default()` before
// iterating `theme.variations`. If a genre pack registers an AudioTheme with
// `variations: []` (a structurally-valid but semantically-malformed bundle),
// the outer mood key is inserted with an empty inner HashMap. `select_variation`
// then hits `Some({})` — the outer `if let Some(mood_variations)` matches, but
// all three inner conditionals (contains_key preferred, contains_key Full,
// iter().next()) fail on the empty map. Control falls out of the Some arm
// WITHOUT entering the else branch (added in the first rework pass), and the
// function returns `preferred` silently.
//
// Reviewer Pass 2 flagged this as [MEDIUM] non-blocking. Per Keith's direction
// "fix all wiring immediately": we close this path in Pass 3 rather than
// tracking as a follow-up story.

/// AudioConfig with an intentionally-malformed theme: the combat theme has
/// `variations: []`, which forces `MusicDirector::new()` to insert `combat` into
/// `themed_tracks` with an empty inner HashMap. Any `select_variation` call
/// classified as combat will hit the `Some(empty_map)` fall-through.
fn audio_config_combat_theme_with_empty_variations() -> AudioConfig {
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
    AudioConfig {
        mood_tracks,
        mood_keywords: HashMap::new(),
        sfx_library: HashMap::new(),
        creature_voice_presets: HashMap::new(),
        mixer: MixerConfig {
            music_volume: 0.6,
            sfx_volume: 0.8,
            voice_volume: 1.0,
            crossfade_default_ms: 3000,
        },
        // Malformed on purpose: the theme bundle has an empty variations vec.
        // Pre-Pass-3 code inserts "combat" into themed_tracks anyway (via
        // or_default) with an empty inner HashMap.
        themes: vec![AudioTheme {
            name: "battle".to_string(),
            mood: "combat".to_string(),
            base_prompt: "epic battle".to_string(),
            variations: vec![],
        }],
        ai_generation: None,
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

#[test]
fn audio_variation_fallback_when_mood_has_empty_variations() {
    let (_guard, mut rx) = fresh_subscriber();

    // Malformed genre pack: combat theme exists but `variations: []`.
    // themed_tracks["combat"] = Some(empty HashMap).
    let config = audio_config_combat_theme_with_empty_variations();
    let director = MusicDirector::new(&config);

    let classification = MoodClassification {
        primary: MoodKey::COMBAT,
        intensity: 0.8,
        confidence: 0.9,
    };
    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    // We don't care what select_variation returns — evaluate() will fall
    // through to mood_tracks. We care that the malformed-theme condition
    // is surfaced on the watcher channel.
    let _ = director.select_variation(&classification, &ctx);

    let events = drain_events(&mut rx);
    let fallback = find_events(&events, "music_director", "variation_fallback");

    assert!(
        !fallback.is_empty(),
        "select_variation() must emit music_director.variation_fallback \
         when `themed_tracks` contains the mood key but the inner HashMap \
         is empty. This is the Some(empty_map) fall-through: the outer \
         `if let Some(mood_variations)` matches, but all three inner \
         emission branches skip because the map is empty, and control \
         falls out of the Some arm WITHOUT entering the else branch. \
         Pre-Pass-3 this returned `preferred` silently. Got {} events total.",
        events.len()
    );

    let evt = &fallback[0];
    assert_eq!(
        evt.fields.get("mood").and_then(serde_json::Value::as_str),
        Some("combat"),
        "variation_fallback must record the mood key with the empty variations"
    );
    // The `reason` field must distinguish this from the other three
    // variation_fallback cases. Accept any of: a dedicated
    // "mood_variations_empty" reason, OR the same "mood_not_in_themed_tracks"
    // reason that the None-case else branch uses (which would be the case
    // if Dev chose the "guard at construction time" fix — skip inserting
    // empty mood keys so they produce None instead of Some(empty)).
    let reason = evt
        .fields
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .expect("variation_fallback must carry a reason field");
    assert!(
        reason == "mood_variations_empty"
            || reason == "mood_not_in_themed_tracks"
            || reason.contains("empty")
            || reason.contains("mood"),
        "reason must identify the empty-variations case — suggested values: \
         'mood_variations_empty' (if Dev adds a new reason label) or \
         'mood_not_in_themed_tracks' (if Dev guards at construction time \
         by skipping empty themes so the key is never inserted), got {:?}",
        reason
    );
    assert_eq!(
        evt.fields
            .get("full_available")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "full_available must be false — the mood's variations vec is empty"
    );
}

// ===========================================================================
// A5 wiring assertions — production code paths reach these subsystems.
// ===========================================================================
//
// These grep-based checks guard against silent removal of the production
// callers. If any of these assertions fail, the subsystem has been
// orphaned and the OTEL events are no longer reachable from production.

#[test]
fn wiring_chargen_reached_by_character_builder_build() {
    // CharacterBuilder::build() is the sole production entry point that
    // drives stats generation, hp_formula evaluation, and backstory
    // composition. If this function stops calling generate_stats /
    // evaluate_hp_formula / backstory composition, all three chargen
    // watcher events become unreachable.
    let src = include_str!("../src/builder.rs");

    assert!(
        src.contains("self.generate_stats("),
        "builder.rs::build() must call self.generate_stats() — without \
         this call the chargen.stats_generated watcher event is unreachable."
    );
    assert!(
        src.contains("evaluate_hp_formula("),
        "builder.rs::build() must call evaluate_hp_formula() — without \
         this call the chargen.hp_formula_evaluated watcher event is \
         unreachable."
    );
    // Backstory composition is an inline match in build() — assert that
    // at least one of the three branches (fragments/tables/fallback) is
    // still present in the source.
    assert!(
        src.contains("backstory_fragments") && src.contains("backstory_tables"),
        "builder.rs::build() must still contain backstory composition \
         logic — without it the chargen.backstory_composed watcher event \
         is unreachable."
    );
}

#[test]
fn wiring_character_builder_reached_by_server_dispatch() {
    // The server reaches CharacterBuilder through dispatch/connect.rs
    // during the chargen WebSocket message flow. Without this reference,
    // no user interaction ever drives chargen in production.
    let src = include_str!("../../sidequest-server/src/dispatch/connect.rs");
    assert!(
        src.contains("CharacterBuilder"),
        "dispatch/connect.rs must reference CharacterBuilder — without \
         this reference the chargen subsystem is not reachable from any \
         production code path and the OTEL events are dead."
    );
}

#[test]
fn wiring_music_director_select_variation_reached_by_evaluate() {
    // select_variation() is called from MusicDirector::evaluate(), which
    // is the production entry point for mood/variation selection. If
    // evaluate() stops calling select_variation(), the AudioVariation
    // fallback watcher event becomes unreachable.
    let src = include_str!("../src/music_director.rs");
    assert!(
        src.contains("self.select_variation(") || src.contains("select_variation(&classification"),
        "music_director.rs::evaluate() must call select_variation() — \
         without this call the music_director.variation_fallback watcher \
         event is unreachable."
    );
}

#[test]
fn wiring_music_director_reached_by_server_dispatch_audio() {
    // The server drives MusicDirector from dispatch/audio.rs during every
    // turn that produces narration. Without this caller, no turn ever
    // reaches select_variation() in production.
    let src = include_str!("../../sidequest-server/src/dispatch/audio.rs");
    assert!(
        src.contains("music_director") && src.contains("MusicDirector"),
        "dispatch/audio.rs must reference MusicDirector — without this \
         reference the audio variation subsystem is not reachable from \
         any production code path and the fallback OTEL event is dead."
    );
}
