//! Story 37-36 — RED wiring tests for party-peer injection reaching the narrator.
//!
//! The unit tests in `sidequest-game/tests/party_peer_identity_story_37_36_tests.rs`
//! prove the data layer works.  That is *necessary but not sufficient* — the
//! whole point of story 37-36 is that the narrator prompt pulls canonical
//! peer identity when describing party-mates.  A `PartyPeer` that lives in
//! `GameSnapshot` but is never read by the prompt assembler is the exact
//! "half-wired feature" this project's CLAUDE.md explicitly forbids.
//!
//! This file closes the wiring gap.  It proves three things:
//!
//!   1. `sidequest_game::format_party_peer_block(&[PartyPeer]) -> String`
//!      exists as a public pure function, and its output contains each
//!      peer's canonical pronouns and name in a narrator-legible form.
//!      This is the **contract** between the data layer and the prompt
//!      layer — any plausible prompt that mentions party-mates goes
//!      through this function.
//!
//!   2. `dispatch/prompt.rs` actually imports and calls
//!      `format_party_peer_block`.  This is a source-scan wiring test —
//!      it fails if Dev adds the formatter but never wires it up.
//!      (Same pattern as the existing `wiring_dispatch_mod_calls_*` tests
//!      that story 37-14's pass 2 rework established.)
//!
//!   3. The turn dispatch path calls `inject_party_peers` at turn start
//!      for multiplayer sessions — so peers are refreshed from the
//!      canonical party roster before the prompt is assembled, not just
//!      populated once at session connect.
//!
//! All three checks are RED until Dev (a) creates the formatter in the
//! game crate, (b) imports + invokes it inside `build_prompt_context`,
//! and (c) calls `inject_party_peers` from the turn dispatch flow.

use std::fs;

use sidequest_game::character::Character;
use sidequest_game::creature_core::{placeholder_edge_pool, CreatureCore};
use sidequest_game::inventory::Inventory;
use sidequest_protocol::NonBlankString;

// ═══════════════════════════════════════════════════════════
// RED compile gate — these MUST be publicly exported from sidequest-game.
// `PartyPeer` also has to be publicly exported for the formatter signature
// to be callable from outside the crate.
// ═══════════════════════════════════════════════════════════
use sidequest_game::{format_party_peer_block, PartyPeer};

// ═══════════════════════════════════════════════════════════
// Fixtures (kept deliberately local — sharing with the game-crate file
// would pull test fixtures across crate boundaries for no real payoff).
// ═══════════════════════════════════════════════════════════

fn char_with_identity(
    name: &str,
    pronouns: &str,
    race: &str,
    class: &str,
    level: u32,
) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
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
        char_class: NonBlankString::new(class).unwrap(),
        race: NonBlankString::new(race).unwrap(),
        pronouns: pronouns.to_string(),
        stats: std::collections::HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

// ═══════════════════════════════════════════════════════════
// 1. Formatter produces narrator-legible text with pronouns
// ═══════════════════════════════════════════════════════════

#[test]
fn format_party_peer_block_includes_each_peers_name_and_pronouns() {
    let peers = vec![
        PartyPeer::from_character(&char_with_identity(
            "Blutka", "he/him", "Ogrekin", "Berserker", 3,
        )),
        PartyPeer::from_character(&char_with_identity(
            "Orin", "they/them", "Human", "Cleric", 3,
        )),
    ];

    let block = format_party_peer_block(&peers);

    // Both names must appear — not "a party member" or other elided shorthand.
    assert!(
        block.contains("Blutka"),
        "formatted block must name Blutka; got: {block}"
    );
    assert!(
        block.contains("Orin"),
        "formatted block must name Orin; got: {block}"
    );

    // Both pronoun strings must appear verbatim — this is the central
    // canonical-identity promise of 37-36.
    assert!(
        block.contains("he/him"),
        "formatted block must carry Blutka's he/him; got: {block}"
    );
    assert!(
        block.contains("they/them"),
        "formatted block must carry Orin's they/them; got: {block}"
    );
}

#[test]
fn format_party_peer_block_empty_peers_produces_empty_or_sentinel() {
    // Solo session, no peers. The formatter must not panic and must not
    // fabricate prose about party members that don't exist.
    let block = format_party_peer_block(&[]);

    // We allow either an empty string or a well-defined empty-state
    // sentinel — but the block must NOT invent peer names.
    assert!(
        !block.contains("Blutka") && !block.contains("Orin"),
        "empty-peers block must not invent content: {block}"
    );
    // And it must be short — an empty formatter should not balloon context.
    assert!(
        block.len() <= 120,
        "empty-peers block must stay small (got {} chars): {block}",
        block.len()
    );
}

// ═══════════════════════════════════════════════════════════
// 2. Source-scan wiring — dispatch/prompt.rs calls the formatter
// ═══════════════════════════════════════════════════════════
//
// Gold standard is an end-to-end integration test that drives
// `build_prompt_context` and inspects the resulting prompt string.
// `build_prompt_context` is `pub(crate)` and needs full AppState
// scaffolding to drive, which is out of proportion for this story.
// We use the same source-scan pattern story 37-14's pass 2 established
// for high-signal / low-scaffolding wiring verification.

#[test]
fn wiring_prompt_rs_imports_and_calls_format_party_peer_block() {
    let prompt_rs = fs::read_to_string("src/dispatch/prompt.rs")
        .expect("read src/dispatch/prompt.rs for source-scan wiring test");

    // The identifier must appear — `pub use`-style re-export games don't
    // satisfy "the prompt assembler uses it". Must actually be called.
    assert!(
        prompt_rs.contains("format_party_peer_block"),
        "dispatch/prompt.rs must call format_party_peer_block to inject \
         canonical peer identity into the narrator prompt — 37-36 wiring"
    );
}

#[test]
fn wiring_prompt_rs_references_party_peers_field() {
    // Belt-and-suspenders: the formatter call-site must be fed from
    // `snapshot.party_peers`, not some stub empty vec. If a Dev wires
    // `format_party_peer_block(&[])` unconditionally, the formatter-use
    // check above would pass, but this one will not.
    let prompt_rs = fs::read_to_string("src/dispatch/prompt.rs")
        .expect("read src/dispatch/prompt.rs for source-scan wiring test");

    assert!(
        prompt_rs.contains("party_peers"),
        "dispatch/prompt.rs must reference snapshot.party_peers so the \
         formatter receives real canonical data, not a stub empty slice"
    );
}

// ═══════════════════════════════════════════════════════════
// 3. Turn-pipeline wiring — inject_party_peers is called at turn start
// ═══════════════════════════════════════════════════════════

#[test]
fn wiring_turn_dispatch_calls_inject_party_peers() {
    // We scan the full dispatch tree rather than hard-coding a single
    // file, because Dev may reasonably place the call in mod.rs,
    // session_sync.rs, or connect.rs — the contract is that the call
    // is reachable in the turn path, not which exact file hosts it.
    let dispatch_dir = "src/dispatch";
    let mut found_in: Option<String> = None;

    for entry in fs::read_dir(dispatch_dir)
        .expect("read src/dispatch/ for turn-pipeline wiring scan")
    {
        let entry = entry.expect("dispatch dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_default();
        if src.contains("inject_party_peers") {
            found_in = Some(path.display().to_string());
            break;
        }
    }

    assert!(
        found_in.is_some(),
        "no file under src/dispatch/ calls inject_party_peers — \
         peer identity will never be refreshed at turn start, and the \
         playtest-3 Blutka/Orin drift remains unfixed"
    );
}
