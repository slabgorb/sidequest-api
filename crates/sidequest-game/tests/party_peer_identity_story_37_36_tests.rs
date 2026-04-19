//! Story 37-36 — RED tests for the party-peer identity packet.
//!
//! **Playtest 3 bug (2026-04-19):** Blutka (he/him in own save) presented as
//! she/her inside Orin's save, because Orin's `GameSnapshot` held zero canonical
//! information about Blutka. The narrator was therefore inventing pronouns per
//! turn, and the invention drifted across sealed-letter turn boundaries between
//! the two players' saves.
//!
//! This file proves the *data layer* of the fix:
//!
//!   1. A `PartyPeer` struct exists, exposes canonical identity fields
//!      (name, pronouns, race, char_class, level), and is constructible
//!      from a `Character`.
//!   2. `GameSnapshot` carries a `party_peers: Vec<PartyPeer>` field with a
//!      serde default so pre-37-36 save files still deserialize.
//!   3. An `inject_party_peers(snapshot, roster, self_name)` function
//!      populates the snapshot with the *other* party members' canonical
//!      identity — and only the other ones (never self).
//!   4. The injection is idempotent — calling it N times produces the same
//!      Vec<PartyPeer>, never duplicates.
//!   5. The injection fails loudly when `self_name` is not in the roster
//!      (no silent empty-roster fallback — see CLAUDE.md "No Silent Fallbacks").
//!   6. The injection emits a `StateTransition` WatcherEvent on the global
//!      telemetry channel with `action=party_peer_inject` and the peer_count,
//!      so the GM panel can verify the subsystem fired (SideQuest OTEL rule).
//!   7. Serde round-trip preserves all canonical fields (sealed-letter durability
//!      at the JSON layer — integration durability through the dispatch pipeline
//!      is covered by the server-crate wiring tests).
//!
//! Compile-gate imports (Dev must create and publicly export these symbols):
//!
//!   * `sidequest_game::PartyPeer`
//!   * `sidequest_game::PartyPeerError`
//!   * `sidequest_game::inject_party_peers`
//!
//! Until those are exported, this file fails to compile — the RED signal.

use std::collections::HashMap;

use serde_json::json;
use sidequest_game::character::Character;
use sidequest_game::creature_core::{placeholder_edge_pool, CreatureCore};
use sidequest_game::inventory::Inventory;
use sidequest_game::state::GameSnapshot;
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEventType};

// ═══════════════════════════════════════════════════════════
// RED compile gate — these MUST be publicly exported from sidequest-game.
// ═══════════════════════════════════════════════════════════
use sidequest_game::{inject_party_peers, PartyPeer, PartyPeerError};

// ═══════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════

/// Build a minimal, fully-formed `Character` with the canonical identity
/// fields the peer packet cares about. The tests use distinct pronoun
/// choices so we can prove field-level fidelity, not just vector length.
fn char_with_identity(
    name: &str,
    pronouns: &str,
    race: &str,
    class: &str,
    level: u32,
) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).expect("test name not blank"),
            description: NonBlankString::new("fixture character").unwrap(),
            personality: NonBlankString::new("fixture personality").unwrap(),
            level,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
            edge: placeholder_edge_pool(),
            acquired_advancements: vec![],
        },
        backstory: NonBlankString::new("fixture backstory").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new(class).expect("test class not blank"),
        race: NonBlankString::new(race).expect("test race not blank"),
        pronouns: pronouns.to_string(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

fn blutka() -> Character {
    char_with_identity("Blutka", "he/him", "Ogrekin", "Berserker", 3)
}

fn orin() -> Character {
    char_with_identity("Orin", "they/them", "Human", "Cleric", 3)
}

fn rux() -> Character {
    char_with_identity("Rux", "she/her", "Goblin", "Scout", 2)
}

// ═══════════════════════════════════════════════════════════
// 1. PartyPeer carries the canonical identity fields
// ═══════════════════════════════════════════════════════════

#[test]
fn party_peer_from_character_copies_canonical_identity() {
    let blutka = blutka();
    let peer = PartyPeer::from_character(&blutka);

    assert_eq!(peer.name.as_str(), "Blutka");
    assert_eq!(peer.pronouns, "he/him");
    assert_eq!(peer.race.as_str(), "Ogrekin");
    assert_eq!(peer.char_class.as_str(), "Berserker");
    assert_eq!(peer.level, 3);
}

#[test]
fn party_peer_from_character_preserves_they_them_pronouns() {
    // Regression guard: early pronoun-handling code coerced non-binary
    // pronouns to empty or defaulted them.  The canonical packet must
    // carry "they/them" exactly as authored.
    let orin = orin();
    let peer = PartyPeer::from_character(&orin);
    assert_eq!(peer.pronouns, "they/them");
}

// ═══════════════════════════════════════════════════════════
// 2. GameSnapshot has a party_peers field, serde-default for old saves
// ═══════════════════════════════════════════════════════════

#[test]
fn game_snapshot_default_has_empty_party_peers() {
    let snap = GameSnapshot::default();
    // Explicit assertion — not merely "the field exists". A vacuous is_empty()
    // check on an always-empty Vec is flagged by rust-review #6.
    assert_eq!(snap.party_peers.len(), 0);
    assert_eq!(snap.party_peers, Vec::<PartyPeer>::new());
}

#[test]
fn legacy_save_json_without_party_peers_deserializes_with_empty_vec() {
    // A pre-37-36 save has no `party_peers` key at all.  Deserialization
    // must succeed with an empty Vec — the serde default — not fail.
    // Uses the same pattern pre-existing compat shims rely on
    // (e.g. `active_tropes`, `encounter`).
    let legacy_json = json!({
        "genre_slug": "heavy_metal",
        "world_slug": "evropi",
        "characters": [],
        "npcs": [],
        "location": "",
        "time_of_day": "",
        "quest_log": {},
        "notes": [],
        "narrative_log": []
        // No party_peers key — this is the legacy-save shape.
    });

    let snap: GameSnapshot =
        serde_json::from_value(legacy_json).expect("legacy save must still deserialize");
    assert_eq!(snap.party_peers.len(), 0);
}

#[test]
fn party_peers_serde_roundtrip_preserves_all_fields() {
    let mut snap = GameSnapshot::default();
    snap.party_peers = vec![
        PartyPeer::from_character(&blutka()),
        PartyPeer::from_character(&orin()),
    ];

    let wire = serde_json::to_string(&snap).expect("serialize");
    let restored: GameSnapshot = serde_json::from_str(&wire).expect("deserialize");

    assert_eq!(restored.party_peers.len(), 2);
    assert_eq!(restored.party_peers[0].name.as_str(), "Blutka");
    assert_eq!(restored.party_peers[0].pronouns, "he/him");
    assert_eq!(restored.party_peers[0].race.as_str(), "Ogrekin");
    assert_eq!(restored.party_peers[0].char_class.as_str(), "Berserker");
    assert_eq!(restored.party_peers[0].level, 3);
    assert_eq!(restored.party_peers[1].pronouns, "they/them");
}

// ═══════════════════════════════════════════════════════════
// 3. inject_party_peers excludes self
// ═══════════════════════════════════════════════════════════

#[test]
fn inject_party_peers_excludes_self_character() {
    let roster = vec![blutka(), orin(), rux()];
    let mut snap = GameSnapshot::default();

    let n = inject_party_peers(&mut snap, &roster, "Orin").expect("inject must succeed");

    // Orin gets two peers (Blutka + Rux), never themselves.
    assert_eq!(n, 2);
    assert_eq!(snap.party_peers.len(), 2);
    let names: Vec<&str> = snap.party_peers.iter().map(|p| p.name.as_str()).collect();
    assert!(!names.contains(&"Orin"), "self must never appear in own peers");
    assert!(names.contains(&"Blutka"));
    assert!(names.contains(&"Rux"));
}

#[test]
fn inject_party_peers_solo_session_produces_zero_peers() {
    let roster = vec![blutka()];
    let mut snap = GameSnapshot::default();

    let n = inject_party_peers(&mut snap, &roster, "Blutka").expect("solo still succeeds");
    assert_eq!(n, 0);
    assert_eq!(snap.party_peers.len(), 0);
}

// ═══════════════════════════════════════════════════════════
// 4. Idempotency — repeated injection does not duplicate
// ═══════════════════════════════════════════════════════════

#[test]
fn inject_party_peers_is_idempotent_no_duplicates() {
    let roster = vec![blutka(), orin(), rux()];
    let mut snap = GameSnapshot::default();

    // Inject three times — a multi-join race, a turn barrier retry, etc.
    inject_party_peers(&mut snap, &roster, "Orin").expect("first inject");
    inject_party_peers(&mut snap, &roster, "Orin").expect("second inject");
    inject_party_peers(&mut snap, &roster, "Orin").expect("third inject");

    assert_eq!(
        snap.party_peers.len(),
        2,
        "party_peers must not accumulate duplicates across repeated injections"
    );
    let names: Vec<&str> = snap.party_peers.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Blutka"));
    assert!(names.contains(&"Rux"));
}

#[test]
fn inject_party_peers_reflects_roster_changes_on_reinject() {
    // Idempotent does NOT mean cached — if the roster changes, reinjection
    // must reflect the new set (a party member leaves, another joins).
    let mut snap = GameSnapshot::default();

    let roster_a = vec![blutka(), orin(), rux()];
    inject_party_peers(&mut snap, &roster_a, "Orin").expect("first inject");
    assert_eq!(snap.party_peers.len(), 2);

    // Rux drops out of the party.
    let roster_b = vec![blutka(), orin()];
    inject_party_peers(&mut snap, &roster_b, "Orin").expect("second inject");
    assert_eq!(snap.party_peers.len(), 1);
    assert_eq!(snap.party_peers[0].name.as_str(), "Blutka");
}

// ═══════════════════════════════════════════════════════════
// 5. Fail-loud on missing self (no silent fallback)
// ═══════════════════════════════════════════════════════════

#[test]
fn inject_party_peers_errors_when_self_not_in_roster() {
    let roster = vec![blutka(), orin()];
    let mut snap = GameSnapshot::default();

    let err = inject_party_peers(&mut snap, &roster, "Nonexistent")
        .expect_err("must fail loudly when self is missing");

    match err {
        PartyPeerError::SelfNotFound(name) => {
            assert_eq!(name, "Nonexistent");
        }
        other => panic!("expected SelfNotFound, got {other:?}"),
    }

    // And critically: the snapshot must not have been half-written.
    assert_eq!(
        snap.party_peers.len(),
        0,
        "failed injection must not leave partial state"
    );
}

// ═══════════════════════════════════════════════════════════
// 6. OTEL — WatcherEvent fires on injection
// ═══════════════════════════════════════════════════════════

#[tokio::test(flavor = "current_thread")]
async fn inject_party_peers_emits_watcher_event() {
    // Initialize the global telemetry channel (idempotent — `OnceLock`
    // `get_or_init` tolerates repeated calls across tests).
    let _tx = init_global_channel();
    let mut rx = subscribe_global().expect("subscribe to global channel");

    let roster = vec![blutka(), orin(), rux()];
    let mut snap = GameSnapshot::default();
    let n = inject_party_peers(&mut snap, &roster, "Orin").expect("inject");
    assert_eq!(n, 2);

    // Drain the channel looking for our injection event. We only care that
    // one of the emitted events matches; other subsystems may also emit.
    let mut found = None;
    for _ in 0..16 {
        match rx.try_recv() {
            Ok(ev) => {
                let action = ev.fields.get("action").and_then(|v| v.as_str()).unwrap_or("");
                if action == "party_peer_inject" {
                    found = Some(ev);
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let ev = found.expect("party_peer_inject WatcherEvent must fire on successful inject");
    assert!(matches!(ev.event_type, WatcherEventType::StateTransition));
    assert_eq!(ev.component, "multiplayer");

    // The peer_count field must reflect the *actual* count — not a stub
    // "unknown"/0 placeholder.  rust-review #3 (hardcoded placeholders).
    let peer_count = ev
        .fields
        .get("peer_count")
        .and_then(|v| v.as_u64())
        .expect("peer_count field must be emitted as a number");
    assert_eq!(peer_count, 2);

    // The self identity must also be present so the GM panel can attribute
    // the injection to the right player.
    let self_name = ev
        .fields
        .get("self_name")
        .and_then(|v| v.as_str())
        .expect("self_name field must be emitted");
    assert_eq!(self_name, "Orin");
}

// ═══════════════════════════════════════════════════════════
// 7. Regression guard — the literal playtest-3 Blutka / Orin bug
// ═══════════════════════════════════════════════════════════

#[test]
fn regression_blutka_pronouns_stable_in_orins_snapshot() {
    // Playtest 3, 2026-04-19: Blutka is authored as he/him in his own save.
    // Orin's save had no record of Blutka, so the narrator improvised
    // she/her — and kept improvising across sealed-letter turns.
    //
    // After 37-36, Orin's snapshot carries a canonical Blutka peer packet
    // with he/him.  This test locks that in.
    let roster = vec![blutka(), orin()];
    let mut orins_snap = GameSnapshot::default();

    inject_party_peers(&mut orins_snap, &roster, "Orin").expect("inject into Orin's snap");

    let blutka_peer = orins_snap
        .party_peers
        .iter()
        .find(|p| p.name.as_str() == "Blutka")
        .expect("Blutka must appear as a peer in Orin's snapshot");
    assert_eq!(
        blutka_peer.pronouns, "he/him",
        "Blutka's canonical pronouns must survive intact in Orin's snapshot"
    );
}
