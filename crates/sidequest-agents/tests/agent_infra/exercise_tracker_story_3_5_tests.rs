//! Story 3-5 RED: Subsystem exercise tracker — agent invocation histogram tests.
//!
//! Tests that the SubsystemTracker correctly:
//!   1. Pre-seeds all 8 expected agents with count 0
//!   2. Increments counts on record()
//!   3. Tracks unknown agents (not in EXPECTED_AGENTS)
//!   4. Emits tracing::info! summary at interval boundaries
//!   5. Detects coverage gaps after threshold turns
//!   6. Emits tracing::warn! for zero-invocation agents
//!   7. Resets cleanly (new instance = fresh state)
//!   8. Uses structured tracing fields: component="watcher", check="subsystem_exercise"
//!
//! RED state: All stubs return defaults / do nothing, so every assertion
//! expecting counts, summaries, or warnings will fail. Dev implements GREEN.

use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::exercise_tracker::{SubsystemTracker, EXPECTED_AGENTS};

// ===========================================================================
// Tracing capture infrastructure (same pattern as 3-3 / 3-4)
// ===========================================================================

/// A captured tracing event with field name-value pairs.
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

/// Layer that captures tracing events for assertion.
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

/// Visitor that collects event field name-value pairs.
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
}

/// Helper: find captured INFO events with component="watcher" and check="subsystem_exercise".
fn exercise_infos(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::INFO
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("subsystem_exercise")
        })
        .collect()
}

/// Helper: find captured WARN events with component="watcher" and check="subsystem_exercise".
fn exercise_warnings(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::WARN
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("subsystem_exercise")
        })
        .collect()
}

// ===========================================================================
// AC1: Expected agents pre-seeded
// ===========================================================================

/// A fresh tracker must contain all 8 expected agents with count 0.
///
/// RED: new() returns empty HashMap — no agents pre-seeded.
#[test]
fn new_tracker_preseeds_all_expected_agents() {
    let tracker = SubsystemTracker::new(5, 10);
    let histogram = tracker.histogram();

    assert_eq!(
        histogram.len(),
        EXPECTED_AGENTS.len(),
        "histogram should contain exactly {} entries (one per expected agent), got {}",
        EXPECTED_AGENTS.len(),
        histogram.len()
    );

    for &agent in EXPECTED_AGENTS {
        assert!(
            histogram.contains_key(agent),
            "expected agent '{}' missing from histogram",
            agent
        );
        assert_eq!(
            histogram[agent], 0,
            "expected agent '{}' should start at 0, got {}",
            agent, histogram[agent]
        );
    }
}

/// Configurable thresholds are stored correctly.
#[test]
fn new_tracker_stores_thresholds() {
    let tracker = SubsystemTracker::new(3, 7);
    assert_eq!(tracker.summary_interval, 3);
    assert_eq!(tracker.gap_threshold, 7);
    assert_eq!(tracker.turn_count, 0);
}

// ===========================================================================
// AC2: Histogram maintained — record() increments counts
// ===========================================================================

/// Recording an agent invocation increments its count.
///
/// RED: record() is a no-op.
#[test]
fn record_increments_agent_count() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("narrator");
    tracker.record("narrator");
    tracker.record("narrator");

    assert_eq!(
        tracker.histogram().get("narrator").copied().unwrap_or(0),
        3,
        "narrator should have count 3 after 3 records"
    );
}

/// Recording increments the turn counter.
///
/// RED: record() is a no-op — turn_count stays 0.
#[test]
fn record_increments_turn_counter() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("narrator");
    tracker.record("world_builder");

    assert_eq!(
        tracker.turn_count, 2,
        "turn_count should be 2 after 2 records"
    );
}

/// Multiple different agents each get their own count.
///
/// RED: record() is a no-op.
#[test]
fn record_tracks_multiple_agents_independently() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("narrator");
    tracker.record("narrator");
    tracker.record("creature_smith");
    tracker.record("troper");
    tracker.record("creature_smith");

    let h = tracker.histogram();
    assert_eq!(h.get("narrator").copied().unwrap_or(0), 2);
    assert_eq!(h.get("creature_smith").copied().unwrap_or(0), 2);
    assert_eq!(h.get("troper").copied().unwrap_or(0), 1);
}

// ===========================================================================
// AC3: Unknown agents tracked
// ===========================================================================

/// Agents not in EXPECTED_AGENTS are still counted (no warning, just tracked).
///
/// RED: record() is a no-op, so unknown agent won't appear.
#[test]
fn unknown_agent_is_tracked() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("mystery_agent");

    assert_eq!(
        tracker
            .histogram()
            .get("mystery_agent")
            .copied()
            .unwrap_or(0),
        1,
        "unknown agent 'mystery_agent' should be tracked with count 1"
    );
}

/// Unknown agents don't interfere with expected agent tracking.
///
/// RED: record() is a no-op.
#[test]
fn unknown_agent_does_not_displace_expected() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("mystery_agent");
    tracker.record("narrator");

    let h = tracker.histogram();
    // Expected agent still there
    assert!(
        h.contains_key("narrator"),
        "narrator should still be in histogram"
    );
    assert_eq!(h.get("narrator").copied().unwrap_or(0), 1);
    // Unknown agent also there
    assert_eq!(h.get("mystery_agent").copied().unwrap_or(0), 1);
    // Total entries: 8 expected + 1 unknown = 9
    assert_eq!(
        h.len(),
        EXPECTED_AGENTS.len() + 1,
        "histogram should have expected agents + 1 unknown"
    );
}

// ===========================================================================
// AC4: Periodic summary — tracing::info! every N turns
// ===========================================================================

/// After exactly summary_interval turns, a tracing::info! with the histogram is emitted.
///
/// RED: record() emits no tracing events.
#[test]
fn emits_summary_at_interval() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        // Record 5 turns (all narrator, doesn't matter for this test)
        for _ in 0..5 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let infos = exercise_infos(&events);

    assert!(
        !infos.is_empty(),
        "expected at least one INFO summary after 5 turns (summary_interval=5)"
    );

    // The summary should include turn_count
    let summary = infos[0];
    assert_eq!(
        summary.field_value("turn_count"),
        Some("5"),
        "summary should report turn_count=5"
    );
}

/// No summary emitted before the interval is reached.
///
/// RED: record() emits no tracing events (passes vacuously, but included for completeness).
#[test]
fn no_summary_before_interval() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        // Record only 4 turns — below interval
        for _ in 0..4 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let infos = exercise_infos(&events);

    assert!(
        infos.is_empty(),
        "no summary should be emitted before reaching summary_interval (4 < 5)"
    );
}

/// Summary is emitted at every multiple of the interval (turn 5, 10, 15...).
///
/// RED: record() emits no tracing events.
#[test]
fn emits_summary_at_every_interval_multiple() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(3, 20);
        // Record 9 turns — should emit at turns 3, 6, and 9
        for _ in 0..9 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let infos = exercise_infos(&events);

    assert_eq!(
        infos.len(),
        3,
        "expected 3 summaries at turns 3, 6, 9 (interval=3, total=9)"
    );
}

// ===========================================================================
// AC5: Coverage gap warning — tracing::warn! for zero-invocation agents
// ===========================================================================

/// After gap_threshold turns, agents with zero invocations trigger a warning.
///
/// RED: record() emits no tracing events.
#[test]
fn warns_about_zero_invocation_agents_after_threshold() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        // Record 10 turns, but only use narrator — 7 other agents have 0 invocations
        for _ in 0..10 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let warns = exercise_warnings(&events);

    assert!(
        !warns.is_empty(),
        "expected coverage gap warnings after 10 turns with 7 unused agents"
    );
}

/// No coverage gap warning before threshold is reached.
///
/// RED: record() emits no tracing events (passes vacuously, included for completeness).
#[test]
fn no_gap_warning_before_threshold() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        // Record 9 turns (below threshold of 10)
        for _ in 0..9 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let warns = exercise_warnings(&events);

    assert!(
        warns.is_empty(),
        "no gap warning should be emitted before gap_threshold (9 < 10)"
    );
}

/// No warning if all expected agents have been invoked at least once.
///
/// RED: record() emits no tracing events (passes vacuously).
#[test]
fn no_gap_warning_when_all_agents_covered() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        // Invoke each expected agent at least once, then fill to 10
        for &agent in EXPECTED_AGENTS {
            tracker.record(agent);
        }
        // 8 agents recorded, need 2 more to reach threshold of 10
        tracker.record("narrator");
        tracker.record("narrator");
    });

    let events = captured.lock().unwrap();
    let warns = exercise_warnings(&events);

    assert!(
        warns.is_empty(),
        "no gap warning expected when all agents have been invoked"
    );
}

// ===========================================================================
// AC6: Structured tracing fields
// ===========================================================================

/// Summary events must include component="watcher" and check="subsystem_exercise".
///
/// RED: record() emits no tracing events.
#[test]
fn summary_has_structured_fields() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        for _ in 0..5 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let infos = exercise_infos(&events);

    assert!(!infos.is_empty(), "expected at least one summary event");
    let evt = infos[0];
    assert_eq!(evt.field_value("component"), Some("watcher"));
    assert_eq!(evt.field_value("check"), Some("subsystem_exercise"));
}

/// Warning events must include component="watcher" and check="subsystem_exercise".
///
/// RED: record() emits no tracing events.
#[test]
fn warning_has_structured_fields() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(5, 10);
        for _ in 0..10 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let warns = exercise_warnings(&events);

    assert!(!warns.is_empty(), "expected at least one warning event");
    let evt = warns[0];
    assert_eq!(evt.field_value("component"), Some("watcher"));
    assert_eq!(evt.field_value("check"), Some("subsystem_exercise"));
}

// ===========================================================================
// AC7: Session reset — new tracker = clean slate
// ===========================================================================

/// A new SubsystemTracker instance has all counts at zero, simulating session reset.
///
/// RED: new() returns empty HashMap.
#[test]
fn fresh_tracker_is_clean_slate() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("narrator");
    tracker.record("narrator");
    tracker.record("narrator");

    // "Reset" by creating a new tracker (as the design specifies)
    let fresh = SubsystemTracker::new(5, 10);

    // Fresh tracker should have all expected agents at 0
    for &agent in EXPECTED_AGENTS {
        assert_eq!(
            fresh.histogram().get(agent).copied().unwrap_or(0),
            0,
            "fresh tracker should have {} at 0",
            agent
        );
    }
    assert_eq!(
        fresh.turn_count, 0,
        "fresh tracker should have turn_count=0"
    );
}

// ===========================================================================
// AC8: uncovered_agents() helper
// ===========================================================================

/// uncovered_agents() returns all expected agents with zero invocations.
///
/// RED: stub returns empty Vec.
#[test]
fn uncovered_agents_returns_zero_invocation_agents() {
    let mut tracker = SubsystemTracker::new(5, 10);
    tracker.record("narrator");
    tracker.record("intent_router");

    let uncovered = tracker.uncovered_agents();

    // 3 agents should be uncovered (all except narrator and intent_router,
    // out of the 5 post-ADR-067 agents).
    assert_eq!(
        uncovered.len(),
        3,
        "expected 3 uncovered agents, got {} — {:?}",
        uncovered.len(),
        uncovered
    );

    // narrator and intent_router should NOT be in the uncovered list
    assert!(
        !uncovered.contains(&"narrator"),
        "narrator was invoked, should not be uncovered"
    );
    assert!(
        !uncovered.contains(&"intent_router"),
        "intent_router was invoked, should not be uncovered"
    );

    // troper should be uncovered (real agent, never invoked in this test)
    assert!(
        uncovered.contains(&"troper"),
        "troper was never invoked, should be uncovered"
    );
}

/// uncovered_agents() returns empty when all agents have been invoked.
///
/// RED: stub returns empty Vec (this passes vacuously — paired with the test above).
#[test]
fn uncovered_agents_empty_when_all_covered() {
    let mut tracker = SubsystemTracker::new(5, 10);
    for &agent in EXPECTED_AGENTS {
        tracker.record(agent);
    }

    let uncovered = tracker.uncovered_agents();
    assert!(
        uncovered.is_empty(),
        "all agents invoked — uncovered should be empty, got {:?}",
        uncovered
    );
}

// ===========================================================================
// Edge case: custom interval = 1 (emit every turn)
// ===========================================================================

/// With summary_interval=1, a summary should be emitted on every single turn.
///
/// RED: record() emits no tracing events.
#[test]
fn summary_every_turn_with_interval_one() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(1, 10);
        tracker.record("narrator");
        tracker.record("narrator");
        tracker.record("narrator");
    });

    let events = captured.lock().unwrap();
    let infos = exercise_infos(&events);

    assert_eq!(
        infos.len(),
        3,
        "with interval=1, should emit summary on every turn (3 turns = 3 summaries)"
    );
}

// ===========================================================================
// Edge case: gap_threshold = gap exactly at threshold boundary
// ===========================================================================

/// Coverage gap warning fires at exactly gap_threshold, not gap_threshold+1.
///
/// RED: record() emits no tracing events.
#[test]
fn gap_warning_fires_at_exact_threshold() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let mut tracker = SubsystemTracker::new(100, 5); // summary far away, gap at 5
        for _ in 0..5 {
            tracker.record("narrator");
        }
    });

    let events = captured.lock().unwrap();
    let warns = exercise_warnings(&events);

    assert!(
        !warns.is_empty(),
        "gap warning should fire at exactly gap_threshold=5"
    );
}
