//! Story 13-1 RED: Deep OTEL instrumentation — game crate subsystem tests.
//!
//! Tests that combat mechanics, trope progression, quest tracking, NPC registry,
//! and music selection emit tracing spans with semantic fields for observability.
//! These subsystems had zero instrumentation prior to this story.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

// ---------------------------------------------------------------------------
// Test infrastructure: span capture layer (same pattern as story 3-1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CapturedSpan {
    id: u64,
    name: String,
    fields: Vec<(String, String)>,
    #[allow(dead_code)]
    target: String,
}

struct SpanCaptureLayer {
    captured: Arc<Mutex<Vec<CapturedSpan>>>,
}

impl SpanCaptureLayer {
    fn new() -> (Self, Arc<Mutex<Vec<CapturedSpan>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                captured: captured.clone(),
            },
            captured,
        )
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for SpanCaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        attrs.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedSpan {
            id: id.into_u64(),
            name: attrs.metadata().name().to_string(),
            fields,
            target: attrs.metadata().target().to_string(),
        });
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        values.record(&mut visitor);

        // Match by span ID to handle nested spans correctly
        let span_id = id.into_u64();
        let mut captured = self.captured.lock().unwrap();
        if let Some(span) = captured.iter_mut().find(|s| s.id == span_id) {
            span.fields.extend(fields);
        }
    }
}

struct FieldCaptureVisitor<'a>(&'a mut Vec<(String, String)>);

impl<'a> tracing::field::Visit for FieldCaptureVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{:?}", value)));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

fn find_span<'a>(spans: &'a [CapturedSpan], name: &str) -> Option<&'a CapturedSpan> {
    spans.iter().find(|s| s.name == name)
}

fn has_field(span: &CapturedSpan, field_name: &str) -> bool {
    span.fields.iter().any(|(name, _)| name == field_name)
}

fn field_value<'a>(span: &'a CapturedSpan, field_name: &str) -> Option<&'a str> {
    span.fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Test helper: build a minimal GameSnapshot
// ---------------------------------------------------------------------------

fn test_snapshot() -> sidequest_game::GameSnapshot {
    // Use Default to avoid breaking when new fields are added
    sidequest_game::GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        location: "The Bazaar".to_string(),
        time_of_day: "dusk".to_string(),
        atmosphere: "tense".to_string(),
        current_region: "central".to_string(),
        discovered_regions: vec!["central".to_string()],
        ..Default::default()
    }
}

// ===========================================================================
// COMBAT MECHANICS — advance_round, log_damage, add_effect, tick_effects
// CombatState advance_round / log_damage / add_effect / tick_effects span tests
// removed — CombatState/StatusEffect were deleted in story 16-2. Combat telemetry
// now flows through StructuredEncounter + ADR-033 confrontation engine. A new
// test against the current encounter.rs API belongs in a followup story.
// ===========================================================================

// ===========================================================================
// TROPE PROGRESSION — tick, activate, resolve
// ===========================================================================

fn test_trope_def() -> sidequest_genre::TropeDefinition {
    use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
    use sidequest_protocol::NonBlankString;

    TropeDefinition {
        id: Some("forbidden_knowledge".to_string()),
        name: NonBlankString::new("Forbidden Knowledge").unwrap(),
        description: Some("Dark secrets surface".to_string()),
        category: "revelation".to_string(),
        triggers: vec!["research".to_string()],
        narrative_hints: vec![],
        tension_level: Some(0.5),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.1,
            rate_per_day: 0.0,
            accelerators: vec!["forbidden".to_string()],
            decelerators: vec!["ignore".to_string()],
            accelerator_bonus: 0.15,
            decelerator_penalty: 0.05,
        }),
        escalation: vec![TropeEscalation {
            at: 0.5,
            event: "Whispers grow louder".to_string(),
            npcs_involved: vec![],
            stakes: "sanity".to_string(),
        }],
        is_abstract: false,
        extends: None,
    }
}

/// TropeEngine::tick must emit a span with trope_count, multiplier, and beats_fired.
#[test]
fn trope_tick_emits_span() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    let mut tropes = vec![TropeState::new("forbidden_knowledge")];
    let defs = vec![test_trope_def()];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _fired = TropeEngine::tick(&mut tropes, &defs);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "trope_tick").expect("Expected a 'trope_tick' span");

    assert!(
        has_field(span, "trope_count"),
        "trope_tick span missing 'trope_count' field"
    );
    assert!(
        has_field(span, "multiplier"),
        "trope_tick span missing 'multiplier' field"
    );
    assert!(
        has_field(span, "beats_fired"),
        "trope_tick span missing 'beats_fired' field"
    );

    assert_eq!(field_value(span, "trope_count"), Some("1"));
}

/// TropeEngine::tick should report beats_fired count when escalation thresholds are crossed.
#[test]
fn trope_tick_reports_beats_fired() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    // Start trope at 0.45 — with rate_per_turn=0.1, one tick takes it to 0.55,
    // crossing the 0.5 threshold
    let mut trope = TropeState::new("forbidden_knowledge");
    trope.set_progression(0.45);
    let mut tropes = vec![trope];
    let defs = vec![test_trope_def()];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _fired = TropeEngine::tick(&mut tropes, &defs);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "trope_tick").expect("Expected a 'trope_tick' span");

    assert_eq!(
        field_value(span, "beats_fired"),
        Some("1"),
        "One beat should have fired (threshold 0.5 crossed)"
    );
}

/// TropeEngine::activate must emit a span with trope_id.
#[test]
fn trope_activate_emits_span() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    let mut tropes: Vec<TropeState> = vec![];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _ts = TropeEngine::activate(&mut tropes, "forbidden_knowledge");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "trope_activate").expect("Expected a 'trope_activate' span");

    assert!(
        has_field(span, "trope_id"),
        "trope_activate span missing 'trope_id' field"
    );
    assert_eq!(field_value(span, "trope_id"), Some("forbidden_knowledge"));
}

/// TropeEngine::resolve must emit a span with trope_id.
#[test]
fn trope_resolve_emits_span() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    let mut tropes = vec![TropeState::new("forbidden_knowledge")];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        TropeEngine::resolve(&mut tropes, "forbidden_knowledge", Some("Secret revealed"));
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "trope_resolve").expect("Expected a 'trope_resolve' span");

    assert!(
        has_field(span, "trope_id"),
        "trope_resolve span missing 'trope_id' field"
    );
    assert_eq!(field_value(span, "trope_id"), Some("forbidden_knowledge"));
}

// Keyword modifier telemetry test removed — apply_keyword_modifiers was deleted.
// Trope progression is now driven by LLM evaluation (TroperAgent::evaluate_triggers).

// ===========================================================================
// QUEST TRACKING — quest_log and quest_updates in apply_world_patch
// ===========================================================================

/// apply_world_patch with quest_updates should emit a quest_update span
/// with quests_added and quests_modified counts.
#[test]
fn quest_update_emits_span() {
    use sidequest_game::WorldStatePatch;

    let mut snapshot = test_snapshot();
    let mut quest_updates = HashMap::new();
    quest_updates.insert("find_the_relic".to_string(), "In progress".to_string());
    quest_updates.insert("rescue_the_healer".to_string(), "Accepted".to_string());

    let patch = WorldStatePatch {
        quest_updates: Some(quest_updates),
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        snapshot.apply_world_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "quest_update").expect("Expected a 'quest_update' span");

    assert!(
        has_field(span, "quests_added"),
        "quest_update span missing 'quests_added' field"
    );

    // Both quests are new (empty quest_log), so both are "added"
    assert_eq!(
        field_value(span, "quests_added"),
        Some("2"),
        "Two quests should be added"
    );
}

/// apply_world_patch with quest_log replacement should emit a quest_update span.
#[test]
fn quest_log_replacement_emits_span() {
    use sidequest_game::WorldStatePatch;

    let mut snapshot = test_snapshot();
    // Pre-populate with an existing quest
    snapshot
        .quest_log
        .insert("find_the_relic".to_string(), "In progress".to_string());

    let mut new_log = HashMap::new();
    new_log.insert("find_the_relic".to_string(), "Complete".to_string());
    new_log.insert("defend_the_wall".to_string(), "Active".to_string());

    let patch = WorldStatePatch {
        quest_log: Some(new_log),
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        snapshot.apply_world_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "quest_update").expect("Expected a 'quest_update' span");

    assert!(
        has_field(span, "quest_count"),
        "quest_update span missing 'quest_count' field"
    );
}

// ===========================================================================
// NPC REGISTRY — merge_patch observability
// ===========================================================================

fn test_npc() -> sidequest_game::Npc {
    use sidequest_game::npc::Npc;
    use sidequest_game::{CreatureCore, Disposition, Inventory};
    use sidequest_protocol::NonBlankString;

    Npc {
        core: CreatureCore {
            name: NonBlankString::new("Grizzled Merchant").unwrap(),
            description: NonBlankString::new("A weathered trader").unwrap(),
            personality: NonBlankString::new("cautious").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(5),
        location: NonBlankString::new("The Bazaar").ok(),
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

/// Npc::merge_patch must emit a span with npc_name and fields_changed.
#[test]
fn npc_merge_patch_emits_span() {
    use sidequest_game::NpcPatch;

    let mut npc = test_npc();
    let patch = NpcPatch {
        name: "Grizzled Merchant".to_string(),
        description: Some("A scarred, weathered trader".to_string()),
        location: Some("The Wastes".to_string()),
        pronouns: Some("he/him".to_string()),
        appearance: Some("Tall with a scar".to_string()),
        personality: None,
        role: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: None,
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        npc.merge_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "npc_merge_patch").expect("Expected a 'npc_merge_patch' span");

    assert!(
        has_field(span, "npc_name"),
        "npc_merge_patch span missing 'npc_name' field"
    );
    assert!(
        has_field(span, "fields_changed"),
        "npc_merge_patch span missing 'fields_changed' field"
    );

    assert_eq!(field_value(span, "npc_name"), Some("Grizzled Merchant"));
}

/// merge_patch should report identity fields locked when they're already set.
#[test]
fn npc_merge_patch_reports_identity_lock() {
    use sidequest_game::NpcPatch;

    let mut npc = test_npc();
    // Set identity fields first
    npc.pronouns = Some("he/him".to_string());
    npc.appearance = Some("Original appearance".to_string());

    // Try to overwrite identity fields — should be blocked
    let patch = NpcPatch {
        name: "Grizzled Merchant".to_string(),
        description: None,
        location: None,
        pronouns: Some("she/her".to_string()),
        appearance: Some("New appearance".to_string()),
        personality: None,
        role: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: None,
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        npc.merge_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "npc_merge_patch").expect("Expected a 'npc_merge_patch' span");

    assert!(
        has_field(span, "identity_fields_locked"),
        "npc_merge_patch span missing 'identity_fields_locked' field — \
         should report when identity overwrite is blocked"
    );
}

// ===========================================================================
// MUSIC DIRECTOR — classify_mood and evaluate
// ===========================================================================

fn test_audio_config() -> sidequest_genre::AudioConfig {
    use sidequest_genre::{AudioConfig, MixerConfig, MoodTrack};

    let mut mood_tracks = HashMap::new();
    mood_tracks.insert(
        "combat".to_string(),
        vec![MoodTrack {
            path: "audio/combat_01.ogg".to_string(),
            title: "Battle Theme".to_string(),
            bpm: 140,
            energy: 0.8,
        }],
    );
    mood_tracks.insert(
        "exploration".to_string(),
        vec![MoodTrack {
            path: "audio/explore_01.ogg".to_string(),
            title: "Wandering".to_string(),
            bpm: 90,
            energy: 0.4,
        }],
    );

    AudioConfig {
        mood_tracks,
        sfx_library: HashMap::new(),
        creature_voice_presets: HashMap::new(),
        mixer: MixerConfig {
            music_volume: 0.7,
            sfx_volume: 0.8,
            voice_volume: 1.0,
            crossfade_default_ms: 2000,
        },
        themes: vec![],
        ai_generation: None,
        mood_keywords: HashMap::new(),
        mixer_defaults: None,
        mood_aliases: HashMap::new(),
        faction_themes: Vec::new(),
    }
}

/// classify_mood must emit a span with mood, intensity, and confidence fields.
#[test]
fn music_classify_mood_emits_span() {
    use sidequest_game::MoodContext;
    use sidequest_game::MusicDirector;

    let director = MusicDirector::new(&test_audio_config());
    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _classification = director.classify_mood("The goblins attack!", &ctx);
    });

    let spans = captured.lock().unwrap();
    let span =
        find_span(&spans, "music_classify_mood").expect("Expected a 'music_classify_mood' span");

    assert!(
        has_field(span, "mood"),
        "music_classify_mood span missing 'mood' field"
    );
    assert!(
        has_field(span, "intensity"),
        "music_classify_mood span missing 'intensity' field"
    );
    assert!(
        has_field(span, "confidence"),
        "music_classify_mood span missing 'confidence' field"
    );

    assert_eq!(
        field_value(span, "mood"),
        Some("combat"),
        "mood should be 'combat' when in_combat context is set"
    );
}

/// evaluate must emit a span with mood, track_id, and action fields.
#[test]
fn music_evaluate_emits_span() {
    use sidequest_game::MoodContext;
    use sidequest_game::MusicDirector;

    let mut director = MusicDirector::new(&test_audio_config());
    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _cue = director.evaluate("The goblins attack!", &ctx);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "music_evaluate").expect("Expected a 'music_evaluate' span");

    assert!(
        has_field(span, "mood"),
        "music_evaluate span missing 'mood' field"
    );
    assert!(
        has_field(span, "track_id"),
        "music_evaluate span missing 'track_id' field"
    );
    assert!(
        has_field(span, "action"),
        "music_evaluate span missing 'action' field"
    );
}

/// evaluate should report no_change when mood hasn't changed.
#[test]
fn music_evaluate_reports_no_change() {
    use sidequest_game::MoodContext;
    use sidequest_game::MusicDirector;

    let mut director = MusicDirector::new(&test_audio_config());
    let ctx = MoodContext {
        in_combat: true,
        ..Default::default()
    };

    // First evaluate to set the mood
    let _ = director.evaluate("The goblins attack!", &ctx);

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        // Second evaluate with same mood — should report no_change
        let _cue = director.evaluate("The battle rages on!", &ctx);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "music_evaluate").expect("Expected a 'music_evaluate' span");

    assert!(
        has_field(span, "mood_changed"),
        "music_evaluate span missing 'mood_changed' field"
    );
    assert_eq!(
        field_value(span, "mood_changed"),
        Some("false"),
        "mood_changed should be false when mood hasn't changed"
    );
}
