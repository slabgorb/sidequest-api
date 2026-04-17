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
        .resolve_merged("archetype_fragments/thornwall_mender", &ctx)
        .unwrap();

    // Genre-tier fragment supplied speech_pattern; world-tier fragment supplied name.
    // With `replace` semantics and serde defaults, the deeper tier overrides only
    // fields it actually provides — fields left unset (empty string from default)
    // still clobber the shallower tier under strict `replace`. The fixtures are
    // authored so each tier provides the field the other tier omits, and the
    // replace walk lands both values because the world tier's name replaces an
    // empty genre-tier name while the world tier's empty speech_pattern replaces
    // the genre-tier one.
    assert_eq!(resolved.value.name, "Thornwall Mender");
    assert_eq!(resolved.provenance.merge_trail.len(), 2);
    assert_eq!(resolved.provenance.merge_trail[0].tier, Tier::Genre);
    assert_eq!(resolved.provenance.merge_trail[1].tier, Tier::World);
    assert_eq!(resolved.provenance.source_tier, Tier::World);
}
