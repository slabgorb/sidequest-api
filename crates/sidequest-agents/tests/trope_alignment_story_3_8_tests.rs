//! Story 3-8 RED: Trope alignment check — beat fired vs narration content.
//!
//! Tests that the trope alignment validator correctly:
//!   1. Extracts keywords from TropeContext (description words 4+ chars + tags)
//!   2. Passes narration that references a beat's keywords (no warning)
//!   3. Flags narration that ignores a fired beat's theme (warning)
//!   4. Matches keywords case-insensitively
//!   5. Matches keywords as substrings (not whole words)
//!   6. Checks each beat independently when multiple beats fire
//!   7. Returns early with no results when no beats fired (empty contexts)
//!   8. Handles gracefully when a beat has no extractable keywords
//!   9. Emits tracing::warn! with component="watcher", check="trope_alignment"
//!  10. Emits tracing::debug! when a beat IS aligned with narration
//!
//! RED state: All stubs return empty Vecs, so every assertion expecting
//! keywords, warnings, or tracing events will fail. The Dev agent implements GREEN.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::patch_legality::ValidationResult;
use sidequest_agents::trope_alignment::{check_trope_alignment, extract_keywords, TropeContext};
use sidequest_agents::turn_record::{PatchSummary, TurnRecord};
use sidequest_game::{
    CombatState, GameSnapshot, StateDelta, TurnManager,
};

// ===========================================================================
// Test infrastructure: mock builders
// ===========================================================================

/// Build a minimal GameSnapshot for testing.
fn mock_game_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![],
        npcs: vec![],
        location: "The Rusty Valve".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "tense and electric".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
    }
}

/// Build a mock StateDelta (private fields, must use serde).
fn mock_state_delta() -> StateDelta {
    serde_json::from_value(serde_json::json!({
        "characters": false,
        "npcs": false,
        "location": false,
        "time_of_day": false,
        "quest_log": false,
        "notes": false,
        "combat": false,
        "chase": false,
        "tropes": false,
        "atmosphere": false,
        "regions": false,
        "routes": false,
        "active_stakes": false,
        "lore": false,
        "new_location": null
    }))
    .expect("mock StateDelta should deserialize")
}

/// Build a TurnRecord with the given narration and beats_fired.
fn make_record(narration: &str, beats_fired: Vec<(String, f32)>) -> TurnRecord {
    TurnRecord {
        turn_id: 1,
        timestamp: Utc::now(),
        player_input: "test action".to_string(),
        classified_intent: Intent::Exploration,
        agent_name: "narrator".to_string(),
        narration: narration.to_string(),
        patches_applied: vec![PatchSummary {
            patch_type: "world".to_string(),
            fields_changed: vec!["notes".to_string()],
        }],
        snapshot_before: mock_game_snapshot(),
        snapshot_after: mock_game_snapshot(),
        delta: mock_state_delta(),
        beats_fired,
        extraction_tier: 1,
        token_count_in: 500,
        token_count_out: 100,
        agent_duration_ms: 1200,
        is_degraded: false,
    }
}

/// Build a TropeContext for the "suspicion" trope at a given beat.
fn suspicion_context(beat_name: &str, threshold: f32, description: &str) -> TropeContext {
    TropeContext {
        trope_name: "suspicion".to_string(),
        beat_name: beat_name.to_string(),
        threshold,
        description: description.to_string(),
        keywords: vec![
            "paranoia".to_string(),
            "distrust".to_string(),
            "secrets".to_string(),
            "betrayal".to_string(),
        ],
    }
}

/// Build a TropeContext for "coming_of_age" trope.
fn coming_of_age_context(beat_name: &str, threshold: f32, description: &str) -> TropeContext {
    TropeContext {
        trope_name: "coming_of_age".to_string(),
        beat_name: beat_name.to_string(),
        threshold,
        description: description.to_string(),
        keywords: vec![
            "maturity".to_string(),
            "choice".to_string(),
            "growth".to_string(),
        ],
    }
}

// ===========================================================================
// Tracing capture infrastructure (reused pattern from stories 3-3/3-4)
// ===========================================================================

#[derive(Debug, Clone)]
struct CapturedEvent {
    fields: Vec<(String, String)>,
    level: tracing::Level,
}

impl CapturedEvent {
    fn field_value(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

struct EventCaptureLayer {
    captured: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl EventCaptureLayer {
    fn new() -> (Self, Arc<Mutex<Vec<CapturedEvent>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                captured: captured.clone(),
            },
            captured,
        )
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for EventCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = EventFieldVisitor(&mut fields);
        event.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedEvent {
            fields,
            level: *event.metadata().level(),
        });
    }
}

struct EventFieldVisitor<'a>(&'a mut Vec<(String, String)>);

impl<'a> tracing::field::Visit for EventFieldVisitor<'a> {
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

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Helper: find captured WARN events with component="watcher" and check="trope_alignment".
fn trope_alignment_warnings(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::WARN
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("trope_alignment")
        })
        .collect()
}

/// Helper: find captured DEBUG events with component="watcher" and check="trope_alignment".
fn trope_alignment_debug_events(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::DEBUG
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("trope_alignment")
        })
        .collect()
}

// ===========================================================================
// AC: Keyword extraction — keywords from description (4+ chars) and tags
// ===========================================================================

#[test]
fn extract_keywords_includes_trope_tags() {
    let ctx = suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    );

    let keywords = extract_keywords(&ctx);

    assert!(
        keywords.contains(&"paranoia".to_string()),
        "Keywords should include trope tag 'paranoia'; got: {:?}",
        keywords
    );
    assert!(
        keywords.contains(&"distrust".to_string()),
        "Keywords should include trope tag 'distrust'; got: {:?}",
        keywords
    );
    assert!(
        keywords.contains(&"betrayal".to_string()),
        "Keywords should include trope tag 'betrayal'; got: {:?}",
        keywords
    );
}

#[test]
fn extract_keywords_includes_description_words_4_plus_chars() {
    let ctx = suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    );

    let keywords = extract_keywords(&ctx);

    // "First" (5), "hints" (5), "someone" (7), "hiding" (6), "something" (9) — all 4+ chars
    // "that" (4), "is" (2) — "that" is 4 chars so included, "is" excluded
    assert!(
        keywords.contains(&"hints".to_string()),
        "Keywords should include description word 'hints' (5 chars); got: {:?}",
        keywords
    );
    assert!(
        keywords.contains(&"hiding".to_string()),
        "Keywords should include description word 'hiding' (6 chars); got: {:?}",
        keywords
    );
    assert!(
        keywords.contains(&"someone".to_string()),
        "Keywords should include description word 'someone' (7 chars); got: {:?}",
        keywords
    );
}

#[test]
fn extract_keywords_excludes_short_words() {
    let ctx = TropeContext {
        trope_name: "test".to_string(),
        beat_name: "test_beat".to_string(),
        threshold: 0.5,
        description: "An old man is sad".to_string(),
        keywords: vec![], // no tags
    };

    let keywords = extract_keywords(&ctx);

    // "An" (2), "old" (3), "man" (3), "is" (2), "sad" (3) — all under 4 chars
    assert!(
        keywords.is_empty(),
        "Should exclude all words under 4 chars; got: {:?}",
        keywords
    );
}

#[test]
fn extract_keywords_lowercased() {
    let ctx = TropeContext {
        trope_name: "test".to_string(),
        beat_name: "test_beat".to_string(),
        threshold: 0.5,
        description: "First Discovery of Hidden Secrets".to_string(),
        keywords: vec!["PARANOIA".to_string()],
    };

    let keywords = extract_keywords(&ctx);

    // All keywords should be lowercased for case-insensitive matching
    for kw in &keywords {
        assert_eq!(
            kw, &kw.to_lowercase(),
            "Keyword '{}' should be lowercased",
            kw
        );
    }
    assert!(
        !keywords.is_empty(),
        "Should have extracted some keywords from description + tags"
    );
}

#[test]
fn extract_keywords_deduplicates() {
    let ctx = TropeContext {
        trope_name: "test".to_string(),
        beat_name: "test_beat".to_string(),
        threshold: 0.5,
        description: "secrets and hidden secrets revealed".to_string(),
        keywords: vec!["secrets".to_string()],
    };

    let keywords = extract_keywords(&ctx);

    let secrets_count = keywords.iter().filter(|k| k.as_str() == "secrets").count();
    assert!(
        secrets_count <= 1,
        "Keyword 'secrets' should appear at most once (dedup); found {} times in {:?}",
        secrets_count,
        keywords
    );
}

#[test]
fn extract_keywords_strips_punctuation() {
    let ctx = TropeContext {
        trope_name: "test".to_string(),
        beat_name: "test_beat".to_string(),
        threshold: 0.5,
        description: "Someone's dark, hidden truth.".to_string(),
        keywords: vec![],
    };

    let keywords = extract_keywords(&ctx);

    // "hidden" should be extracted without trailing punctuation
    assert!(
        keywords.iter().any(|k| k == "hidden"),
        "Should extract 'hidden' without punctuation; got: {:?}",
        keywords
    );
    // No keyword should contain punctuation
    for kw in &keywords {
        assert!(
            kw.chars().all(|c| c.is_alphanumeric()),
            "Keyword '{}' should not contain punctuation",
            kw
        );
    }
}

// ===========================================================================
// AC: Alignment check — beat with matching narration (no warning)
// ===========================================================================

#[test]
fn alignment_match_found_no_warning() {
    let record = make_record(
        "A creeping sense of paranoia settles over the group as whispers echo.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Narration contains keyword 'paranoia' — should NOT produce a warning; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: Alignment gap — beat with unrelated narration (warning emitted)
// ===========================================================================

#[test]
fn alignment_gap_detected_warning_emitted() {
    let record = make_record(
        "The hero walked through the sunny meadow and picked some flowers.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        !warnings.is_empty(),
        "Narration has no thematic overlap with 'suspicion' beat — should produce a warning"
    );
}

#[test]
fn alignment_gap_warning_mentions_beat_and_trope() {
    let record = make_record(
        "The hero walked through the sunny meadow and picked some flowers.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let has_descriptive_warning = results.iter().any(|r| match r {
        ValidationResult::Warning(msg) => {
            msg.contains("seeds_of_doubt") && msg.contains("suspicion")
        }
        _ => false,
    });
    assert!(
        has_descriptive_warning,
        "Warning message should mention both beat name 'seeds_of_doubt' and trope name 'suspicion'; got: {:?}",
        results
    );
}

#[test]
fn alignment_gap_warning_includes_threshold_percentage() {
    let record = make_record(
        "The hero walked through the sunny meadow.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let has_threshold = results.iter().any(|r| match r {
        ValidationResult::Warning(msg) => msg.contains("25%"),
        _ => false,
    });
    assert!(
        has_threshold,
        "Warning message should include threshold as percentage '25%'; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: Case insensitive — keyword matching ignores case
// ===========================================================================

#[test]
fn alignment_match_case_insensitive() {
    let record = make_record(
        "PARANOIA gripped the survivors as shadows moved in the darkness.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Case-insensitive match: 'PARANOIA' in narration should match keyword 'paranoia'; got warnings: {:?}",
        results
    );
}

#[test]
fn alignment_match_mixed_case_narration() {
    let record = make_record(
        "She couldn't shake the growing DiStRuSt between the factions.",
        vec![("suspicion".to_string(), 0.50)],
    );
    let contexts = vec![suspicion_context(
        "evidence_found",
        0.50,
        "A concrete clue or discovery that confirms suspicion",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Mixed-case 'DiStRuSt' should match keyword 'distrust'; got warnings: {:?}",
        results
    );
}

// ===========================================================================
// AC: Substring matching — keywords matched as substrings, not whole words
// ===========================================================================

#[test]
fn alignment_match_keyword_as_substring() {
    // "distrustful" contains the keyword "distrust" as a substring
    let record = make_record(
        "The distrustful villagers refused to speak with the outsiders.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "'distrustful' contains keyword 'distrust' — should NOT produce a warning; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: No beats fired — validator returns early, no results
// ===========================================================================

#[test]
fn empty_trope_contexts_returns_no_results() {
    let record = make_record(
        "The hero explored the ancient ruins without incident.",
        vec![],
    );
    let contexts: Vec<TropeContext> = vec![];

    let results = check_trope_alignment(&record, &contexts);

    assert!(
        results.is_empty(),
        "No beats fired — should return empty results; got: {:?}",
        results
    );
}

#[test]
fn empty_trope_contexts_emits_no_tracing() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_record("The hero rested by the campfire.", vec![]);
    let contexts: Vec<TropeContext> = vec![];

    tracing::subscriber::with_default(subscriber, || {
        check_trope_alignment(&record, &contexts);
    });

    let events = captured.lock().unwrap();
    let warnings = trope_alignment_warnings(&events);
    let debugs = trope_alignment_debug_events(&events);

    assert!(
        warnings.is_empty(),
        "No beats fired — should emit no tracing warnings"
    );
    assert!(
        debugs.is_empty(),
        "No beats fired — should emit no tracing debug events"
    );
}

// ===========================================================================
// AC: Multiple beats — each checked independently
// ===========================================================================

#[test]
fn multiple_beats_all_aligned_no_warnings() {
    let record = make_record(
        "Paranoia spread as the young hero made a difficult choice about growth and maturity.",
        vec![
            ("suspicion".to_string(), 0.25),
            ("coming_of_age".to_string(), 0.50),
        ],
    );
    let contexts = vec![
        suspicion_context(
            "seeds_of_doubt",
            0.25,
            "First hints that someone is hiding something",
        ),
        coming_of_age_context(
            "trial_of_will",
            0.50,
            "A defining moment that tests resolve",
        ),
    ];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Both beats have matching keywords in narration — no warnings; got: {:?}",
        results
    );
}

#[test]
fn multiple_beats_one_aligned_one_gap() {
    // Narration mentions "paranoia" (suspicion aligned) but nothing about growth/maturity
    let record = make_record(
        "Paranoia crept in as the old man shuffled cards at the tavern table.",
        vec![
            ("suspicion".to_string(), 0.25),
            ("coming_of_age".to_string(), 0.50),
        ],
    );
    let contexts = vec![
        suspicion_context(
            "seeds_of_doubt",
            0.25,
            "First hints that someone is hiding something",
        ),
        coming_of_age_context(
            "trial_of_will",
            0.50,
            "A defining moment that tests resolve",
        ),
    ];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "Only 'coming_of_age' should produce a warning (suspicion is aligned); got: {:?}",
        results
    );

    // Verify the warning is about the right trope
    let warning_about_coming_of_age = results.iter().any(|r| match r {
        ValidationResult::Warning(msg) => msg.contains("coming_of_age"),
        _ => false,
    });
    assert!(
        warning_about_coming_of_age,
        "Warning should be about 'coming_of_age', not 'suspicion'; got: {:?}",
        results
    );
}

#[test]
fn multiple_beats_all_gaps() {
    // Narration has no connection to either trope
    let record = make_record(
        "The merchant displayed a fine collection of silks and spices from the east.",
        vec![
            ("suspicion".to_string(), 0.75),
            ("coming_of_age".to_string(), 0.25),
        ],
    );
    let contexts = vec![
        suspicion_context(
            "confrontation",
            0.75,
            "Direct accusation or tense standoff",
        ),
        coming_of_age_context(
            "first_challenge",
            0.25,
            "An early test of character and courage",
        ),
    ];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert_eq!(
        warnings.len(),
        2,
        "Both beats have zero keyword overlap — should produce 2 warnings; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: Graceful handling — beat with no extractable keywords
// ===========================================================================

#[test]
fn beat_with_empty_keywords_and_short_description_no_panic() {
    let record = make_record(
        "The hero pressed on through the storm.",
        vec![("mystery".to_string(), 0.50)],
    );
    // Description has only short words, no tags — extract_keywords returns empty
    let contexts = vec![TropeContext {
        trope_name: "mystery".to_string(),
        beat_name: "a_clue".to_string(),
        threshold: 0.50,
        description: "A new clue is found".to_string(), // all words < 4 chars except "found" (5) and "clue" (4)
        keywords: vec![], // no tags
    }];

    // Should not panic — gracefully handle sparse keyword sets
    let results = check_trope_alignment(&record, &contexts);

    // With very few keywords, we expect either a warning (no match) or ok (match).
    // The important thing is it doesn't panic.
    assert!(
        results.len() <= 1,
        "Single beat should produce at most one result; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: Single keyword match is sufficient to pass (no false negatives)
// ===========================================================================

#[test]
fn single_keyword_match_sufficient_no_warning() {
    // Only one keyword ("secrets") appears in narration, but that's enough
    let record = make_record(
        "There were secrets buried beneath the old temple floor.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Single keyword match ('secrets') should be sufficient — no warning; got: {:?}",
        results
    );
}

// ===========================================================================
// AC: Description-derived keyword match (not just tags)
// ===========================================================================

#[test]
fn alignment_match_via_description_keyword() {
    // Narration contains "hiding" — a word from the beat description, not the tags
    let record = make_record(
        "She noticed the stranger was hiding something behind his back.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.is_empty(),
        "Keyword 'hiding' from description should count as a match; got warnings: {:?}",
        results
    );
}

// ===========================================================================
// AC: Structured tracing fields — component="watcher", check="trope_alignment"
// ===========================================================================

#[test]
fn gap_emits_tracing_warn_with_correct_fields() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_record(
        "The merchant sold fine wares at the market.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    tracing::subscriber::with_default(subscriber, || {
        check_trope_alignment(&record, &contexts);
    });

    let events = captured.lock().unwrap();
    let warnings = trope_alignment_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Should emit tracing::warn! for alignment gap"
    );

    let warn_event = warnings[0];
    assert_eq!(
        warn_event.field_value("component"),
        Some("watcher"),
        "Warning should have component='watcher'"
    );
    assert_eq!(
        warn_event.field_value("check"),
        Some("trope_alignment"),
        "Warning should have check='trope_alignment'"
    );
}

#[test]
fn gap_tracing_warn_includes_trope_and_beat_fields() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_record(
        "The hero ate a sandwich.",
        vec![("suspicion".to_string(), 0.75)],
    );
    let contexts = vec![suspicion_context(
        "confrontation",
        0.75,
        "Direct accusation or tense standoff",
    )];

    tracing::subscriber::with_default(subscriber, || {
        check_trope_alignment(&record, &contexts);
    });

    let events = captured.lock().unwrap();
    let warnings = trope_alignment_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Should emit tracing::warn! for alignment gap"
    );

    let warn_event = warnings[0];
    assert!(
        warn_event
            .field_value("trope")
            .map_or(false, |v| v.contains("suspicion")),
        "Warning should include trope name 'suspicion'; fields: {:?}",
        warn_event.fields
    );
    assert!(
        warn_event
            .field_value("beat")
            .map_or(false, |v| v.contains("confrontation")),
        "Warning should include beat name 'confrontation'; fields: {:?}",
        warn_event.fields
    );
}

#[test]
fn match_emits_tracing_debug_with_correct_fields() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_record(
        "A deep sense of paranoia filled the room.",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    tracing::subscriber::with_default(subscriber, || {
        check_trope_alignment(&record, &contexts);
    });

    let events = captured.lock().unwrap();
    let debugs = trope_alignment_debug_events(&events);

    assert!(
        !debugs.is_empty(),
        "Should emit tracing::debug! when beat is aligned with narration"
    );

    let debug_event = debugs[0];
    assert_eq!(
        debug_event.field_value("component"),
        Some("watcher"),
        "Debug event should have component='watcher'"
    );
    assert_eq!(
        debug_event.field_value("check"),
        Some("trope_alignment"),
        "Debug event should have check='trope_alignment'"
    );
}

#[test]
fn aligned_beat_does_not_emit_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_record(
        "Betrayal hung in the air as distrust spread through the camp.",
        vec![("suspicion".to_string(), 0.50)],
    );
    let contexts = vec![suspicion_context(
        "evidence_found",
        0.50,
        "A concrete clue or discovery that confirms suspicion",
    )];

    tracing::subscriber::with_default(subscriber, || {
        check_trope_alignment(&record, &contexts);
    });

    let events = captured.lock().unwrap();
    let warnings = trope_alignment_warnings(&events);

    assert!(
        warnings.is_empty(),
        "Aligned beat should NOT emit tracing::warn!; got: {:?}",
        warnings
            .iter()
            .map(|e| &e.fields)
            .collect::<Vec<_>>()
    );
}

// ===========================================================================
// Edge case: empty narration with beats fired
// ===========================================================================

#[test]
fn empty_narration_with_beat_produces_warning() {
    let record = make_record(
        "",
        vec![("suspicion".to_string(), 0.25)],
    );
    let contexts = vec![suspicion_context(
        "seeds_of_doubt",
        0.25,
        "First hints that someone is hiding something",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        !warnings.is_empty(),
        "Empty narration cannot match any keywords — should produce a warning"
    );
}

// ===========================================================================
// Edge case: threshold at boundaries (0.0, 1.0)
// ===========================================================================

#[test]
fn threshold_at_100_percent_still_checked() {
    let record = make_record(
        "The hero had a lovely picnic in the park.",
        vec![("suspicion".to_string(), 1.0)],
    );
    let contexts = vec![suspicion_context(
        "truth_revealed",
        1.0,
        "The secret comes out, for better or worse",
    )];

    let results = check_trope_alignment(&record, &contexts);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        !warnings.is_empty(),
        "Beat at 100% threshold should still be checked for alignment; got: {:?}",
        results
    );

    // Verify the warning mentions 100%
    let mentions_threshold = results.iter().any(|r| match r {
        ValidationResult::Warning(msg) => msg.contains("100%"),
        _ => false,
    });
    assert!(
        mentions_threshold,
        "Warning for 1.0 threshold should mention '100%'; got: {:?}",
        results
    );
}
