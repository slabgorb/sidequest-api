//! Scene harness wiring gate — exercises the full YAML → GameSnapshot →
//! SqliteStore → reload pipeline against real genre pack data.
//!
//! This test is the canonical wiring check for `sidequest-fixture`. If it
//! passes, the fixture loader has:
//!  - parsed the YAML
//!  - validated the world slug against the real genre pack
//!  - deserialized the `Character` block through `NonBlankString`
//!  - resolved `encounter.type` against a real `ConfrontationDef`
//!  - hydrated a `StructuredEncounter` with beats from that def
//!  - persisted and reloaded the snapshot through `SqliteStore`
//!  - preserved the encounter across the round trip
//!
//! Covers success criterion #2 from `docs/plans/scene-harness.md` for the
//! fixture crate; the dev route HTTP wiring is tested in the sidequest-server
//! integration suite.

use std::path::PathBuf;

use sidequest_fixture::{hydrate_fixture, load_fixture, save_path_for};
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_genre::{GenreCode, GenreLoader};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = {repo}/sidequest-api/crates/sidequest-fixture
    // parent()^3 = {repo} (orchestrator root)
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(|p| p.parent()) // sidequest-api/
        .and_then(|p| p.parent()) // repo root
        .expect("could not resolve repo root from CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn load_real_pack(genre: &str) -> sidequest_genre::GenrePack {
    let packs_path = repo_root().join("sidequest-content").join("genre_packs");
    assert!(
        packs_path.exists(),
        "genre packs path missing: {}",
        packs_path.display()
    );
    let loader = GenreLoader::new(vec![packs_path]);
    let code = GenreCode::new(genre).expect("valid genre code");
    loader.load(&code).expect("genre pack loads")
}

#[test]
fn poker_fixture_hydrates_against_real_genre_pack() {
    let fixture_path = repo_root()
        .join("scenarios")
        .join("fixtures")
        .join("poker.yaml");
    let fixture = load_fixture(&fixture_path).expect("poker.yaml loads");
    assert_eq!(fixture.genre, "spaghetti_western");
    assert_eq!(fixture.world, "dust_and_lead");
    assert_eq!(fixture.encounter.confrontation_type, "poker");

    let pack = load_real_pack("spaghetti_western");
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

    // The ConfrontationDef for poker must supply the beat library — the
    // fixture itself authors no beats. If this assertion fires, either the
    // genre pack rules.yaml has drifted or `from_confrontation_def` has
    // stopped seeding the confrontation payload from the def.
    let def = pack
        .rules
        .confrontations
        .iter()
        .find(|d| d.confrontation_type == "poker")
        .expect("spaghetti_western rules.yaml must define `- type: poker`");
    assert!(
        !def.beats.is_empty(),
        "poker ConfrontationDef must declare at least one beat"
    );
}

#[test]
fn dogfight_fixture_hydrates_against_real_genre_pack() {
    let fixture_path = repo_root()
        .join("scenarios")
        .join("fixtures")
        .join("dogfight.yaml");
    let fixture = load_fixture(&fixture_path).expect("dogfight.yaml loads");
    let pack = load_real_pack("space_opera");
    let snapshot = hydrate_fixture(&fixture, &pack).expect("dogfight fixture hydrates");
    let enc = snapshot.encounter.expect("dogfight encounter present");
    assert_eq!(enc.encounter_type, "dogfight");
}

#[test]
fn negotiation_fixture_hydrates_against_real_genre_pack() {
    let fixture_path = repo_root()
        .join("scenarios")
        .join("fixtures")
        .join("negotiation.yaml");
    let fixture = load_fixture(&fixture_path).expect("negotiation.yaml loads");
    let pack = load_real_pack("pulp_noir");
    let snapshot = hydrate_fixture(&fixture, &pack).expect("negotiation fixture hydrates");
    let enc = snapshot.encounter.expect("negotiation encounter present");
    assert_eq!(enc.encounter_type, "negotiation");
}

#[test]
fn save_reload_round_trip_preserves_encounter() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // SIDEQUEST_HOME drives save_path_for — isolate from real saves.
    std::env::set_var("SIDEQUEST_HOME", tmp.path());

    let fixture_path = repo_root()
        .join("scenarios")
        .join("fixtures")
        .join("poker.yaml");
    let fixture = load_fixture(&fixture_path).unwrap();
    let pack = load_real_pack("spaghetti_western");
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
