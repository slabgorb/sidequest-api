//! Party-peer identity packet — canonical identity for other party members.
//!
//! Story 37-36 (playtest 3 bug, 2026-04-19): Blutka (he/him in own save)
//! drifted to she/her in Orin's save because Orin's `GameSnapshot` held zero
//! canonical information about Blutka. The narrator therefore invented
//! pronouns per turn, and the invention drifted across sealed-letter turn
//! boundaries between the two player saves.
//!
//! `PartyPeer` is the minimal canonical identity packet we inject into every
//! player's `GameSnapshot`, so the narrator prompt always has the authoritative
//! name / pronouns / race / class / level for every other party member —
//! regardless of which player's perception layer is active.
//!
//! The perception layer stays POV-centered (e.g. "you see a hooded stranger");
//! the packet here is the physical/canonical ground truth that doesn't change
//! across POVs.

use serde::{Deserialize, Serialize};

use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{Severity, WatcherEventBuilder, WatcherEventType};

use crate::character::Character;
use crate::state::GameSnapshot;

/// Canonical identity fields for another party member — the minimal packet
/// the narrator needs to reference a party-mate without inventing pronouns.
///
/// Intentionally omits POV/perception fields (backstory, hooks, narrative
/// state, known facts). Those belong in the perception layer; this type
/// carries only the physical/canonical identity that must stay stable across
/// every player's save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyPeer {
    /// Display name of the party member (canonical).
    pub name: NonBlankString,
    /// Player-authored pronouns (e.g. "he/him", "they/them"). Stored verbatim.
    #[serde(default)]
    pub pronouns: String,
    /// Race (canonical).
    pub race: NonBlankString,
    /// Character class (canonical).
    pub char_class: NonBlankString,
    /// Level (canonical).
    pub level: u32,
}

impl PartyPeer {
    /// Extract the canonical identity packet from a full `Character`.
    ///
    /// Only the five canonical fields are copied. POV data (backstory, hooks,
    /// narrative state, stats, known facts, etc.) is deliberately dropped —
    /// it belongs on the owning player's snapshot, not every peer's snapshot.
    pub fn from_character(c: &Character) -> Self {
        Self {
            name: c.core.name.clone(),
            pronouns: c.pronouns.clone(),
            race: c.race.clone(),
            char_class: c.char_class.clone(),
            level: c.core.level,
        }
    }
}

/// Errors from `inject_party_peers`.
///
/// `#[non_exhaustive]` per the Rust review checklist — the error set is
/// expected to grow as the injection pathway learns about more invariants
/// (e.g. duplicate names in roster, invalid self handle).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PartyPeerError {
    /// `self_name` was not present in the canonical party roster.
    ///
    /// This is the "fail-loud" branch: callers must not silently swallow this
    /// into an empty `party_peers` Vec, because a missing self indicates a
    /// coordination bug higher up (e.g. the dispatch layer handing the wrong
    /// roster) that would otherwise present as invented pronouns.
    #[error("self character '{0}' not found in canonical party roster")]
    SelfNotFound(String),
}

/// Inject canonical peer packets into `snapshot.party_peers`.
///
/// Semantics:
///
/// * `self_name` is the display name of the player character whose snapshot
///   this is.  That character is **excluded** from `party_peers` — a player
///   never becomes their own peer.
/// * The call is **idempotent**: it clears `snapshot.party_peers` and rebuilds
///   it from `roster`.  Multi-call invocations (turn-barrier retries,
///   reconnect races) never duplicate.
/// * The call is also **roster-reflective**: if a party member leaves the
///   roster between invocations, they disappear from `party_peers` on the
///   next call.  Stale entries are not preserved.
/// * If `self_name` is not in `roster`, the function returns
///   `Err(PartyPeerError::SelfNotFound)` and leaves `snapshot.party_peers`
///   untouched — no partial state is written.
/// * On success, a `StateTransition` WatcherEvent fires on the global telemetry
///   channel with `action = "party_peer_inject"`, the real `peer_count`, and
///   the real `self_name`.  The GM panel depends on this to verify the peer
///   identity subsystem actually engaged on a given turn.
pub fn inject_party_peers(
    snapshot: &mut GameSnapshot,
    roster: &[Character],
    self_name: &str,
) -> Result<usize, PartyPeerError> {
    if !roster.iter().any(|c| c.core.name.as_str() == self_name) {
        return Err(PartyPeerError::SelfNotFound(self_name.to_string()));
    }

    snapshot.party_peers.clear();
    for c in roster {
        if c.core.name.as_str() != self_name {
            snapshot.party_peers.push(PartyPeer::from_character(c));
        }
    }
    let peer_count = snapshot.party_peers.len();

    WatcherEventBuilder::new("multiplayer", WatcherEventType::StateTransition)
        .severity(Severity::Info)
        .field("action", "party_peer_inject")
        .field("peer_count", peer_count as u64)
        .field("self_name", self_name)
        .send();

    Ok(peer_count)
}

/// Render a narrator-legible block describing every peer's canonical identity.
///
/// This is the single contract between the peer-identity data layer and the
/// narrator prompt assembler. The block is spliced directly into the prompt
/// so the narrator can reference party-mates by name without ever inventing
/// pronouns.
///
/// An empty peer slice returns an empty string — the caller's responsibility
/// to decide whether a sentinel belongs in the prompt at all. We deliberately
/// do not fabricate "no party members" prose here; the narrator should not be
/// told about party members that do not exist.
pub fn format_party_peer_block(peers: &[PartyPeer]) -> String {
    if peers.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(peers.len() + 1);
    lines.push("Party-peer identity (canonical — do not paraphrase pronouns):".to_string());
    for p in peers {
        lines.push(format!(
            "- {} ({}): {} {}, Level {}",
            p.name, p.pronouns, p.race, p.char_class, p.level,
        ));
    }
    lines.join("\n")
}
