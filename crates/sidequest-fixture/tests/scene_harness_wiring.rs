//! Scene harness wiring gate — exercises the full YAML → GameSnapshot →
//! SqliteStore → reload pipeline against a **self-contained test-only
//! genre pack** under `tests/fixtures/scene_harness_pack/`.
//!
//! Decoupled from shipped content (per the pattern established by
//! commit 4e612d24). If this test passes, the fixture loader has:
//!  - parsed the scene fixture YAML
//!  - validated the world slug against the test pack
//!  - deserialized the `Character` block through `NonBlankString`
//!  - resolved `encounter.type` against a real `ConfrontationDef`
//!  - hydrated a `StructuredEncounter` with beats from that def
//!  - persisted and reloaded the snapshot through `SqliteStore`
//!  - preserved the encounter across the round trip
//!
//! Covers success criterion #2 from `docs/plans/scene-harness.md`. The
//! dev-route HTTP wiring continues to be tested in the sidequest-server
//! integration suite against shipped content.

use std::path::PathBuf;

use sidequest_fixture::{hydrate_fixture, load_fixture, save_path_for};
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_genre::{GenreCode, GenreLoader};

/// Root of the `tests/fixtures/` directory for this crate.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Load the self-contained test pack. No coupling to sidequest-content.
fn load_test_pack() -> sidequest_genre::GenrePack {
    let loader = GenreLoader::new(vec![fixtures_dir()]);
    let code = GenreCode::new("scene_harness_pack").expect("valid genre code");
    loader.load(&code).expect("test pack loads")
}

fn scene_fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

#[test]
fn poker_fixture_hydrates_against_test_pack() {
    let fixture = load_fixture(&scene_fixture_path("poker.yaml")).expect("poker.yaml loads");
    assert_eq!(fixture.genre, "scene_harness_pack");
    assert_eq!(fixture.world, "scene_harness_world");
    assert_eq!(fixture.encounter.confrontation_type, "poker");

    let pack = load_test_pack();
    let snapshot = hydrate_fixture(&fixture, &pack).expect("poker fixture hydrates");

    let enc = snapshot
        .encounter
        .as_ref()
        .expect("hydrated snapshot must carry an active encounter");
    assert_eq!(
        enc.encounter_type, "poker",
        "encounter type must match the ConfrontationDef key"
    );
    assert_eq!(
        snapshot.characters.len(),
        1,
        "fixture is single-character; party[1..] is out of scope"
    );

    let def = pack
        .rules
        .confrontations
        .iter()
        .find(|d| d.confrontation_type == "poker")
        .expect("test pack rules.yaml must define `- type: poker`");
    assert!(
        !def.beats.is_empty(),
        "poker ConfrontationDef must declare at least one beat"
    );
}

#[test]
fn dogfight_fixture_hydrates_against_test_pack() {
    let fixture = load_fixture(&scene_fixture_path("dogfight.yaml")).expect("dogfight.yaml loads");
    let pack = load_test_pack();
    let snapshot = hydrate_fixture(&fixture, &pack).expect("dogfight fixture hydrates");
    let enc = snapshot.encounter.expect("dogfight encounter present");
    assert_eq!(enc.encounter_type, "dogfight");
}

#[test]
fn negotiation_fixture_hydrates_against_test_pack() {
    let fixture =
        load_fixture(&scene_fixture_path("negotiation.yaml")).expect("negotiation.yaml loads");
    let pack = load_test_pack();
    let snapshot = hydrate_fixture(&fixture, &pack).expect("negotiation fixture hydrates");
    let enc = snapshot.encounter.expect("negotiation encounter present");
    assert_eq!(enc.encounter_type, "negotiation");
}

#[test]
fn save_reload_round_trip_preserves_encounter() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // SIDEQUEST_HOME drives save_path_for — isolate from real saves.
    std::env::set_var("SIDEQUEST_HOME", tmp.path());

    let fixture = load_fixture(&scene_fixture_path("poker.yaml")).unwrap();
    let pack = load_test_pack();
    let snapshot = hydrate_fixture(&fixture, &pack).unwrap();

    let save_path = save_path_for(&fixture.genre, &fixture.world, &fixture.player_name);
    std::fs::create_dir_all(save_path.parent().unwrap()).unwrap();

    // Write
    let store = SqliteStore::open(save_path.to_str().unwrap()).unwrap();
    store.init_session(&fixture.genre, &fixture.world).unwrap();
    store.save(&snapshot).unwrap();
    drop(store);

    // Re-open and reload — this is the path dispatch_connect takes on
    // returning-player restore. If the encounter is None here, the scene
    // harness fails its core promise.
    let store = SqliteStore::open(save_path.to_str().unwrap()).unwrap();
    let loaded = store
        .load()
        .expect("load ok")
        .expect("saved session exists");
    let enc = loaded
        .snapshot
        .encounter
        .as_ref()
        .expect("reloaded snapshot MUST carry the encounter — dispatch_connect relies on this");
    assert_eq!(enc.encounter_type, "poker");
    assert_eq!(loaded.snapshot.characters.len(), 1);

    // INSERT OR REPLACE overwrite test — plan criterion #3: running
    // `sidequest-fixture load poker` twice must not error.
    let store = SqliteStore::open(save_path.to_str().unwrap()).unwrap();
    store
        .save(&snapshot)
        .expect("second save overwrites cleanly");
}
