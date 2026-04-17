//! OTEL `content.resolve` span emission for the four-tier resolver.
//!
//! Every successful call to [`crate::resolver::Resolver::resolve_merged`]
//! emits one `content.resolve` span carrying the provenance of the resolved
//! value. The span is the observability spine of the layered content model
//! per the spec — if the GM panel cannot see a resolution, it did not happen
//! through the framework.

use crate::resolver::{Provenance, Tier};

/// Emit the `content.resolve` span with the full attribute set per the spec.
///
/// Attributes emitted:
/// - `content.axis` — semantic axis name (archetype / audio / image / trope / scenario)
/// - `content.field_path` — dotted or slashed path identifying the resolved field
/// - `content.genre` — genre pack identifier
/// - `content.world` — world identifier (empty string if absent)
/// - `content.culture` — culture identifier (empty string if absent)
/// - `content.source_tier` — final winning tier (global / genre / world / culture)
/// - `content.source_file` — path of the file that supplied the final value
/// - `content.merge_trail_len` — number of tiers that contributed to the merge
/// - `content.elapsed_us` — wall-clock duration of the resolve call in microseconds
pub fn emit_content_resolve_span(
    axis: &str,
    field_path: &str,
    genre: &str,
    world: Option<&str>,
    culture: Option<&str>,
    provenance: &Provenance,
    elapsed_us: u64,
) {
    let tier_str = match provenance.source_tier {
        Tier::Global => "global",
        Tier::Genre => "genre",
        Tier::World => "world",
        Tier::Culture => "culture",
    };
    tracing::info_span!(
        "content.resolve",
        otel.name = "content.resolve",
        content.axis = axis,
        content.field_path = field_path,
        content.genre = genre,
        content.world = world.unwrap_or(""),
        content.culture = culture.unwrap_or(""),
        content.source_tier = tier_str,
        content.source_file = %provenance.source_file.display(),
        content.merge_trail_len = provenance.merge_trail.len(),
        content.elapsed_us = elapsed_us,
    )
    .in_scope(|| ());
}
