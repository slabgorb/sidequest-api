//! Story 37-11 — Review-rework RED tests.
//!
//! Reviewer rejected the first GREEN handoff with six blocking findings. Four of
//! them are testable (OTEL tracing at the merge site, comment integrity, downstream
//! wiring). These tests enforce the fixes.
//!
//! Blocking findings covered here:
//!   - H2: OTEL regression at assemble_turn merge site (no source-tagged tracing)
//!   - H3: "preprocessor (keyword-based)" comment misrepresents the actual fallback
//!         (the preprocessor functions `classify_action`/`rewrite_action` are not
//!         called in production — fallback is `Default::default()`)
//!   - H4: Downstream wiring gap — `result.action_rewrite` / `result.action_flags`
//!         set by `assemble_turn` but no production consumer in `dispatch/mod.rs`
//!
//! Not covered here (dev cleanup only — no failing tests):
//!   - H1: `cargo fmt --check` failures (mechanical)
//!   - H5: "ALWAYS include this" prompt contract vs `Option<T>` (doc/design choice)
//!   - H6: Stale `creature_smith` references (doc fixes, 4 locations)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::orchestrator::{ActionFlags, ActionRewrite, NarratorExtraction};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};

// ============================================================================
// Event capture infrastructure — same pattern as story 3-5 telemetry tests.
// We need on_event (not on_new_span) because assemble_turn's override sites
// use `tracing::info!(...)` which records an EVENT, not a span.
// ============================================================================

#[derive(Debug, Clone)]
struct CapturedEvent {
    name: String,
    fields: Vec<(String, String)>,
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
            name: event.metadata().name().to_string(),
            fields,
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
}

// ============================================================================
// Helpers
// ============================================================================

fn minimal_extraction() -> NarratorExtraction {
    NarratorExtraction {
        prose: "You step into the ruins.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("exploration".to_string()),
        personality_events: vec![],
        scene_intent: Some("Exploration".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        beat_selections: vec![],
        confrontation: None,
        location: None,
        affinity_progress: vec![],
        gold_change: None,
    }
}

fn run_with_capture<F: FnOnce()>(f: F) -> Vec<CapturedEvent> {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, f);
    let guard = captured.lock().unwrap();
    guard.clone()
}

/// Find events emitted from the `assemble_turn` merge point that describe
/// which source won for action_rewrite. Accepts any of:
///   - event with `source = "narrator"` and a name/field referring to action_rewrite
///   - event with `source = "fallback"` (or "preprocessor_fallback" / "default_fallback")
///     referring to action_rewrite
fn action_rewrite_source_events<'a>(events: &'a [CapturedEvent]) -> Vec<&'a CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            let mentions_action_rewrite = e.name.contains("action_rewrite")
                || e.fields
                    .iter()
                    .any(|(k, v)| k.contains("action_rewrite") || v.contains("action_rewrite"));
            mentions_action_rewrite && e.field_value("source").is_some()
        })
        .collect()
}

fn action_flags_source_events<'a>(events: &'a [CapturedEvent]) -> Vec<&'a CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            let mentions_action_flags = e.name.contains("action_flags")
                || e.fields
                    .iter()
                    .any(|(k, v)| k.contains("action_flags") || v.contains("action_flags"));
            mentions_action_flags && e.field_value("source").is_some()
        })
        .collect()
}

// ============================================================================
// H2: OTEL source-tagged tracing at assemble_turn merge point.
//
// Five other override sites in the same function (scene_mood, scene_intent,
// visual_scene, quest_updates, etc.) emit `tracing::info!(source = "tool_call",
// ..., "assemble.override.<field>")`. The action_rewrite/action_flags pair at
// lines 212-213 are the only merge sites with NO source-tagged tracing. The
// GM panel's OTEL dashboard cannot verify which path won on any given turn.
//
// CLAUDE.md (sidequest-api): "Every backend fix that touches a subsystem MUST
// add OTEL watcher events so the GM panel can verify the fix is working."
// ============================================================================

/// When the narrator provides action_rewrite, assemble_turn must emit an OTEL
/// event tagging the source as "narrator" so the GM panel can observe that
/// the LLM classification won over the fallback.
#[test]
fn assemble_turn_emits_source_narrator_when_action_rewrite_present() {
    let mut extraction = minimal_extraction();
    extraction.action_rewrite = Some(ActionRewrite {
        you: "You inspect the runes".to_string(),
        named: "Kael inspects the runes".to_string(),
        intent: "inspect runes".to_string(),
    });
    let fallback_rewrite = ActionRewrite::default();
    let fallback_flags = ActionFlags::default();

    let events = run_with_capture(|| {
        let _ = assemble_turn(
            extraction,
            fallback_rewrite,
            fallback_flags,
            ToolCallResults::default(),
        );
    });

    let source_events = action_rewrite_source_events(&events);
    assert!(
        !source_events.is_empty(),
        "assemble_turn must emit at least one OTEL event referencing \
         action_rewrite with a `source` field when the narrator provides a value. \
         No such event found. All events: {:?}",
        events
    );

    let narrator_source_events: Vec<_> = source_events
        .iter()
        .filter(|e| e.field_value("source") == Some("narrator"))
        .collect();
    assert!(
        !narrator_source_events.is_empty(),
        "assemble_turn must emit source=\"narrator\" when extraction.action_rewrite \
         is Some(...). Got source-tagged events but none had source=\"narrator\": {:?}",
        source_events
    );
}

/// When the narrator omits action_rewrite, assemble_turn must emit an OTEL
/// event tagging the source as the fallback path (any of "fallback",
/// "preprocessor_fallback", "default_fallback") so the GM panel can detect
/// narrator non-compliance.
#[test]
fn assemble_turn_emits_source_fallback_when_action_rewrite_absent() {
    let extraction = minimal_extraction(); // action_rewrite = None
    let fallback_rewrite = ActionRewrite {
        you: "You look around".to_string(),
        named: "Kael looks around".to_string(),
        intent: "look around".to_string(),
    };
    let fallback_flags = ActionFlags::default();

    let events = run_with_capture(|| {
        let _ = assemble_turn(
            extraction,
            fallback_rewrite,
            fallback_flags,
            ToolCallResults::default(),
        );
    });

    let source_events = action_rewrite_source_events(&events);
    assert!(
        !source_events.is_empty(),
        "assemble_turn must emit at least one OTEL event referencing \
         action_rewrite with a `source` field when the narrator OMITS a value \
         (so the GM panel can detect narrator non-compliance). No such event found."
    );

    let fallback_source_events: Vec<_> = source_events
        .iter()
        .filter(|e| {
            matches!(
                e.field_value("source"),
                Some("fallback") | Some("preprocessor_fallback") | Some("default_fallback")
            )
        })
        .collect();
    assert!(
        !fallback_source_events.is_empty(),
        "assemble_turn must emit source=\"fallback\" (or \"preprocessor_fallback\" / \
         \"default_fallback\") when extraction.action_rewrite is None. \
         Got source-tagged events but none used a fallback marker: {:?}",
        source_events
    );
}

/// Same contract as action_rewrite — action_flags merge site must emit
/// source-tagged OTEL on the narrator-wins path.
#[test]
fn assemble_turn_emits_source_narrator_when_action_flags_present() {
    let mut extraction = minimal_extraction();
    extraction.action_flags = Some(ActionFlags {
        is_power_grab: true,
        references_inventory: false,
        references_npc: false,
        references_ability: true,
        references_location: false,
    });
    let fallback_rewrite = ActionRewrite::default();
    let fallback_flags = ActionFlags::default();

    let events = run_with_capture(|| {
        let _ = assemble_turn(
            extraction,
            fallback_rewrite,
            fallback_flags,
            ToolCallResults::default(),
        );
    });

    let source_events = action_flags_source_events(&events);
    assert!(
        !source_events.is_empty(),
        "assemble_turn must emit at least one OTEL event referencing \
         action_flags with a `source` field when the narrator provides a value. \
         No such event found."
    );

    let narrator_source_events: Vec<_> = source_events
        .iter()
        .filter(|e| e.field_value("source") == Some("narrator"))
        .collect();
    assert!(
        !narrator_source_events.is_empty(),
        "assemble_turn must emit source=\"narrator\" when extraction.action_flags \
         is Some(...). Got source-tagged events but none had source=\"narrator\": {:?}",
        source_events
    );
}

/// Same contract as action_rewrite fallback — action_flags merge site must emit
/// source-tagged OTEL on the fallback path.
#[test]
fn assemble_turn_emits_source_fallback_when_action_flags_absent() {
    let extraction = minimal_extraction(); // action_flags = None
    let fallback_rewrite = ActionRewrite::default();
    let fallback_flags = ActionFlags {
        is_power_grab: false,
        references_inventory: true,
        references_npc: true,
        references_ability: true,
        references_location: true,
    };

    let events = run_with_capture(|| {
        let _ = assemble_turn(
            extraction,
            fallback_rewrite,
            fallback_flags,
            ToolCallResults::default(),
        );
    });

    let source_events = action_flags_source_events(&events);
    assert!(
        !source_events.is_empty(),
        "assemble_turn must emit at least one OTEL event referencing \
         action_flags with a `source` field when the narrator OMITS a value. \
         No such event found."
    );

    let fallback_source_events: Vec<_> = source_events
        .iter()
        .filter(|e| {
            matches!(
                e.field_value("source"),
                Some("fallback") | Some("preprocessor_fallback") | Some("default_fallback")
            )
        })
        .collect();
    assert!(
        !fallback_source_events.is_empty(),
        "assemble_turn must emit source=\"fallback\" (or \"preprocessor_fallback\" / \
         \"default_fallback\") when extraction.action_flags is None. \
         Got source-tagged events but none used a fallback marker: {:?}",
        source_events
    );
}

// ============================================================================
// H3: Comment integrity — the merge-site comments currently claim the fallback
// is a "preprocessor (keyword-based)" path, but `classify_action` and
// `rewrite_action` from `tools::preprocessors` are not called in any production
// code path. The actual fallback is `ActionRewrite::default()` /
// `ActionFlags::default()`. The comment lies to future maintainers.
//
// Dev has two options to pass this test:
//   (a) Update the comments on assemble_turn.rs lines 8, 61, 211 to match the
//       real fallback (remove "keyword-based" framing).
//   (b) Actually wire `classify_action` / `rewrite_action` as the fallback
//       (then the comments become truthful).
// ============================================================================

/// The inline merge-site comment and module/fn docstrings in assemble_turn.rs
/// must not describe the fallback as "keyword-based" unless the mechanical
/// preprocessor (`classify_action` / `rewrite_action` in tools::preprocessors)
/// is actually wired into production.
#[test]
fn assemble_turn_comments_do_not_misrepresent_fallback_as_keyword_based() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/src/tools/assemble_turn.rs", manifest_dir);
    let src = std::fs::read_to_string(&path).expect("assemble_turn.rs must exist");

    // If the source still claims the fallback is "keyword-based", then either
    // the comment must be corrected OR the mechanical preprocessor must
    // actually be wired. We let Dev choose.
    let claims_keyword_fallback = src.contains("keyword-based")
        || src.contains("keyword matching")
        || src.contains("keyword matcher");
    let wires_mechanical_preprocessor =
        src.contains("classify_action") || src.contains("rewrite_action");

    assert!(
        !claims_keyword_fallback || wires_mechanical_preprocessor,
        "assemble_turn.rs comments claim the fallback is \"keyword-based\" but \
         the mechanical preprocessor (classify_action / rewrite_action from \
         tools::preprocessors) is not imported or called in this file. \
         Either update the comments (lines 8, 61, 211) to describe the actual \
         fallback (ActionRewrite::default() / ActionFlags::default()) OR wire \
         the mechanical preprocessor as the real fallback."
    );
}

// ============================================================================
// H4: Downstream wiring — `result.action_rewrite` / `result.action_flags` are
// populated by assemble_turn but nothing in `crates/sidequest-server/src/`
// reads them back. The story AC-3 says "narrator values flow into ActionResult"
// which is met at the assembler level, but CLAUDE.md's rule "Verify Wiring,
// Not Just Existence" requires a production consumer. Without one, the fix is
// a Potemkin wire — the values are assembled and dropped.
//
// Dev has three paths to pass this test:
//   (a) Promote `result.action_flags.is_power_grab` (and friends) back into
//       the `preprocessed` variable consumed at dispatch/mod.rs:859 (wish
//       engine) and :985 (`.you` prompt zone).
//   (b) Rewire the specific consumers to read from `result` directly.
//   (c) Emit a `WatcherEventBuilder` that records the action_rewrite / flags
//       values from `result` on the GM panel (making the consumption
//       observable even if no behavioral change happens this turn).
// ============================================================================

/// `dispatch/mod.rs` must contain at least one production reference to
/// `result.action_rewrite` / `result.action_flags` — either a field access,
/// a WatcherEvent emission carrying those values, or an assignment that
/// promotes them back into `preprocessed`. A compile-time presence check
/// is sufficient for the RED phase; Dev decides the shape of the wire.
#[test]
fn dispatch_has_production_consumer_for_result_action_rewrite_or_flags() {
    // Read dispatch/mod.rs from the sibling crate.
    // CARGO_MANIFEST_DIR is crates/sidequest-agents; sidequest-server is a sibling.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/../sidequest-server/src/dispatch/mod.rs", manifest_dir);
    let src = std::fs::read_to_string(&path).expect("dispatch/mod.rs must exist");

    // Strip line comments so we don't match in comments only.
    let code_only: String = src
        .lines()
        .map(|l| {
            if let Some(idx) = l.find("//") {
                &l[..idx]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Any ONE of these patterns in non-comment code indicates real wiring:
    //   - `.action_rewrite`  (field access on an ActionResult)
    //   - `.action_flags`    (field access on an ActionResult)
    //   - `"action_rewrite"` (WatcherEvent field name string literal)
    //   - `"action_flags"`   (WatcherEvent field name string literal)
    let wires_via_field_access =
        code_only.contains(".action_rewrite") || code_only.contains(".action_flags");
    let wires_via_watcher_event =
        code_only.contains("\"action_rewrite\"") || code_only.contains("\"action_flags\"");

    assert!(
        wires_via_field_access || wires_via_watcher_event,
        "No production consumer of result.action_rewrite / result.action_flags \
         found in sidequest-server/src/dispatch/mod.rs. The narrator's classification \
         is assembled into ActionResult by assemble_turn but never read downstream. \
         Fix one of:\n\
         (a) promote result.action_flags.is_power_grab into `preprocessed` before \
             downstream reads (restores wish-engine wiring);\n\
         (b) rewire the specific consumers (dispatch/mod.rs:859, :985) to read from \
             `result` directly;\n\
         (c) emit a WatcherEventBuilder with action_rewrite / action_flags fields \
             from `result` so the GM panel can observe the values.\n\
         See CLAUDE.md: \"Verify Wiring, Not Just Existence\" and \"Every Test \
         Suite Needs a Wiring Test.\""
    );
}
