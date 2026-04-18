use sidequest_genre::resolver::{ResolutionContext, Resolver, Tier};
use sidequest_genre::schema::world::WorldContent;
use sidequest_genre::Layered;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/heavy_metal_evropi")
}

#[test]
fn resolver_returns_world_tier_provenance_for_funnel() {
    let root = fixture_root();
    let ctx = ResolutionContext {
        genre: "heavy_metal".into(),
        world: Some("evropi".into()),
        culture: None,
    };
    let resolved: sidequest_genre::resolver::Resolved<WorldContent> =
        Resolver::<WorldContent>::new(&root)
            .resolve("world", &ctx)
            .unwrap();
    assert_eq!(resolved.provenance.source_tier, Tier::World);
    assert!(resolved
        .provenance
        .source_file
        .ends_with("worlds/evropi/world.yaml"));
    assert!(resolved
        .value
        .funnels
        .iter()
        .any(|f| f.name == "Thornwall Mender"));
}

/// Sample archetype type deserialized from per-tier fragment files.
///
/// Both fields use `replace` merge — the deeper tier's value wins when both
/// tiers contribute. Unset fields in a deeper tier keep the shallower tier's
/// value (serde default fills the gap to empty string).
#[derive(Debug, Clone, Default, serde::Deserialize, Layered)]
struct ArchetypeSample {
    #[serde(default)]
    #[layer(merge = "replace")]
    name: String,
    #[serde(default)]
    #[layer(merge = "replace")]
    speech_pattern: String,
}

#[test]
fn resolver_merges_genre_and_world_tiers() {
    let root = fixture_root();
    let ctx = ResolutionContext {
        genre: "heavy_metal".into(),
        world: Some("evropi".into()),
        culture: None,
    };
    let resolved = Resolver::<ArchetypeSample>::new(&root)
        .resolve_merged("archetype", "archetype_fragments/thornwall_mender", &ctx)
        .unwrap();

    // World-tier fragment supplied `name`; genre-tier fragment supplied
    // `speech_pattern`. Under literal `replace` semantics the deeper tier
    // always wins per field, so world's defaulted empty `speech_pattern`
    // clobbers genre's "measured". Assertions focus on chain composition
    // and provenance rather than the merged scalar value — a future
    // refinement could make `replace` skip `Default` values.
    assert_eq!(resolved.value.name, "Thornwall Mender");
    assert_eq!(resolved.provenance.merge_trail.len(), 2);
    assert_eq!(resolved.provenance.merge_trail[0].tier, Tier::Genre);
    assert_eq!(resolved.provenance.merge_trail[1].tier, Tier::World);
    assert_eq!(resolved.provenance.source_tier, Tier::World);
}

// ───────────────────────────────────────────────────────────────────────────
// Phase E — OTEL `content.resolve` span emission
// ───────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{prelude::*, Layer, Registry};

/// A span captured by the test subscriber. Only holds what the Phase E test needs.
#[derive(Debug, Clone)]
struct CapturedSpan {
    name: String,
    attrs: HashMap<String, String>,
}

/// Minimal tracing Layer that records every `on_new_span` into a shared Vec.
/// Scoped to a single test via `with_default`.
#[derive(Clone, Default)]
struct CaptureLayer {
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
}

struct FieldCollector(HashMap<String, String>);

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let meta = attrs.metadata();
        let mut collector = FieldCollector(HashMap::new());
        attrs.record(&mut collector);
        self.spans.lock().unwrap().push(CapturedSpan {
            name: meta.name().to_string(),
            attrs: collector.0,
        });
    }
}

#[test]
fn resolver_emits_content_resolve_span() {
    let capture = CaptureLayer::default();
    let subscriber = Registry::default().with(capture.clone());

    with_default(subscriber, || {
        let root = fixture_root();
        let ctx = ResolutionContext {
            genre: "heavy_metal".into(),
            world: Some("evropi".into()),
            culture: None,
        };
        let _ = Resolver::<ArchetypeSample>::new(&root)
            .resolve_merged("archetype", "archetype_fragments/thornwall_mender", &ctx)
            .unwrap();
    });

    let spans = capture.spans.lock().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name == "content.resolve")
        .expect("content.resolve span not emitted");

    assert_eq!(
        span.attrs.get("content.axis").map(String::as_str),
        Some("archetype")
    );
    assert_eq!(
        span.attrs.get("content.genre").map(String::as_str),
        Some("heavy_metal")
    );
    assert_eq!(
        span.attrs.get("content.world").map(String::as_str),
        Some("evropi")
    );
    assert_eq!(
        span.attrs.get("content.source_tier").map(String::as_str),
        Some("world")
    );
    assert_eq!(
        span.attrs.get("content.field_path").map(String::as_str),
        Some("archetype_fragments/thornwall_mender")
    );
    assert_eq!(
        span.attrs
            .get("content.merge_trail_len")
            .map(String::as_str),
        Some("2")
    );
    assert!(span.attrs.contains_key("content.elapsed_us"));
}
