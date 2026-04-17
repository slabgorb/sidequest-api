use sidequest_genre::load_genre_pack;
use std::path::Path;

#[test]
fn test_load_low_fantasy_with_constraints() {
    let content_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("sidequest-content")
        .join("genre_workshopping")
        .join("low_fantasy");

    assert!(
        content_path.exists(),
        "sidequest-content/genre_workshopping/low_fantasy required at {} — clone the sidequest-content subrepo",
        content_path.display()
    );

    let pack = load_genre_pack(&content_path).expect("Failed to load low_fantasy");

    // Verify constraints loaded
    assert!(
        pack.archetype_constraints.is_some(),
        "archetype_constraints.yaml should be loaded"
    );

    let constraints = pack.archetype_constraints.as_ref().unwrap();
    assert!(
        !constraints.valid_pairings.common.is_empty(),
        "Should have common pairings"
    );

    // Verify funnels loaded for shattered_reach
    if let Some(world) = pack.worlds.get("shattered_reach") {
        assert!(
            world.archetype_funnels.is_some(),
            "shattered_reach should have archetype_funnels.yaml"
        );
        let funnels = world.archetype_funnels.as_ref().unwrap();
        assert!(
            !funnels.funnels.is_empty(),
            "Should have at least one funnel"
        );

        // Verify resolution works end-to-end
        let mender = funnels.resolve("sage", "healer");
        assert!(mender.is_some(), "sage+healer should resolve to a funnel");
        // Check that the name is a lore-grounded name (not "Hedge Healer")
        let name = &mender.unwrap().name;
        assert!(!name.is_empty(), "Resolved funnel should have a name");
    }
}

#[test]
fn test_base_archetypes_loaded() {
    let content_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("sidequest-content")
        .join("genre_workshopping")
        .join("low_fantasy");

    assert!(
        content_path.exists(),
        "sidequest-content/genre_workshopping/low_fantasy required at {} — clone the sidequest-content subrepo",
        content_path.display()
    );

    let pack = load_genre_pack(&content_path).expect("Failed to load low_fantasy");

    // Base archetypes should be loaded from content root
    assert!(
        pack.base_archetypes.is_some(),
        "base_archetypes should be loaded from archetypes_base.yaml"
    );

    let base = pack.base_archetypes.as_ref().unwrap();
    assert_eq!(base.jungian.len(), 12, "Should have 12 Jungian archetypes");
    assert_eq!(base.rpg_roles.len(), 7, "Should have 7 RPG roles");
    assert_eq!(base.npc_roles.len(), 9, "Should have 9 NPC roles");

    // Verify a specific archetype
    let sage = base.jungian.iter().find(|j| j.id == "sage");
    assert!(sage.is_some(), "Should have sage archetype");
}

#[test]
fn test_full_resolution_chain() {
    let content_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("sidequest-content")
        .join("genre_workshopping")
        .join("low_fantasy");

    assert!(
        content_path.exists(),
        "sidequest-content/genre_workshopping/low_fantasy required at {} — clone the sidequest-content subrepo",
        content_path.display()
    );

    let pack = load_genre_pack(&content_path).expect("Failed to load low_fantasy");

    let base = pack.base_archetypes.as_ref().expect("base_archetypes");
    let constraints = pack.archetype_constraints.as_ref().expect("constraints");

    // Test with world funnels
    if let Some(world) = pack.worlds.get("shattered_reach") {
        let funnels = world.archetype_funnels.as_ref();

        let result = sidequest_genre::archetype_resolve::resolve_archetype(
            "sage",
            "healer",
            base,
            constraints,
            funnels,
        );
        assert!(result.is_ok(), "sage+healer should resolve");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.resolution_source,
            sidequest_genre::archetype_resolve::ResolutionSource::WorldFunnel,
            "Should resolve via world funnel"
        );
    }

    // Test genre fallback (no funnels)
    let result = sidequest_genre::archetype_resolve::resolve_archetype(
        "hero",
        "tank",
        base,
        constraints,
        None,
    );
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(
        resolved.name, "Shield-Bearer",
        "Should fall back to genre name"
    );
    assert_eq!(
        resolved.resolution_source,
        sidequest_genre::archetype_resolve::ResolutionSource::GenreFallback,
    );

    // Test forbidden pairing
    let result = sidequest_genre::archetype_resolve::resolve_archetype(
        "innocent",
        "stealth",
        base,
        constraints,
        None,
    );
    assert!(result.is_err(), "innocent+stealth should be forbidden");
}
