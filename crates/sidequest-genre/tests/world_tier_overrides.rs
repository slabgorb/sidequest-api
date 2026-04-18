//! Wiring test for world-tier openings.yaml and char_creation.yaml overrides.
//!
//! Guards the interim surgical override ahead of the full Phase 2 layered-
//! content migration (see `docs/superpowers/specs/2026-04-17-layered-content-
//! model-design.md`). Verifies that when a world ships its own openings.yaml
//! or char_creation.yaml, the loader surfaces them on `World` — so consumers
//! can prefer world-tier content and stop genre-tier named instances (Long
//! Foundry covenant openings) from leaking into every world's chargen.

use sidequest_genre::load_genre_pack;
use std::path::PathBuf;

fn heavy_metal_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("sidequest-content")
        .join("genre_packs")
        .join("heavy_metal")
}

#[test]
fn evropi_openings_load_from_world_tier() {
    let pack = load_genre_pack(&heavy_metal_path()).expect("heavy_metal loads");
    let evropi = pack.worlds.get("evropi").expect("evropi world present");

    assert!(
        !evropi.openings.is_empty(),
        "evropi must ship its own openings.yaml so the narrator does not fall \
         back on genre-tier hooks that reference Long Foundry-named factions"
    );

    // Canon check: every evropi opening must reference evropi-native content,
    // never the Long Foundry-specific Perault/Thessil/Refuser names that
    // previously lived at genre tier.
    for hook in &evropi.openings {
        let combined = format!("{} {} {}", hook.situation, hook.tone, hook.first_turn_seed);
        for leaked in ["Perault", "Thessil", "Refuser"] {
            assert!(
                !combined.contains(leaked),
                "evropi opening '{}' references Long Foundry name '{}'",
                hook.id,
                leaked
            );
        }
    }
}

#[test]
fn long_foundry_openings_relocated_to_world_tier() {
    let pack = load_genre_pack(&heavy_metal_path()).expect("heavy_metal loads");
    let lf = pack
        .worlds
        .get("long_foundry")
        .expect("long_foundry world present");

    assert!(
        !lf.openings.is_empty(),
        "long_foundry must carry the Perault/Collector openings at world tier, \
         not pass through the genre-tier fallback"
    );

    let ids: Vec<&str> = lf.openings.iter().map(|h| h.id.as_str()).collect();
    assert!(
        ids.contains(&"covenant_called_due"),
        "long_foundry openings missing covenant_called_due (was at genre tier)"
    );
}

#[test]
fn genre_tier_openings_hold_only_cross_world_content() {
    let pack = load_genre_pack(&heavy_metal_path()).expect("heavy_metal loads");

    for hook in &pack.openings {
        let combined = format!("{} {}", hook.situation, hook.first_turn_seed);
        for leaked in ["Perault", "Thessil"] {
            assert!(
                !combined.contains(leaked),
                "genre-tier opening '{}' still references Long Foundry name '{}' — \
                 belongs at worlds/long_foundry/openings.yaml",
                hook.id,
                leaked
            );
        }
    }
}
