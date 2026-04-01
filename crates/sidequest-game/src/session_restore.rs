//! Session restore — extract full character state from a loaded snapshot.
//!
//! Story 18-9: dispatch_connect() previously extracted only hp/max_hp/level/xp
//! from the saved character, leaving inventory and known_facts at defaults.
//! This module provides a testable extraction function that captures ALL
//! character state needed by the dispatch loop.

use crate::inventory::Inventory;
use crate::known_fact::KnownFact;
use crate::state::GameSnapshot;

/// All character state fields needed by the dispatch loop after session restore.
pub struct RestoredCharacterState {
    /// Character display name.
    pub character_name: String,
    /// Current hit points.
    pub hp: i32,
    /// Maximum hit points.
    pub max_hp: i32,
    /// Armor class.
    pub ac: i32,
    /// Character level.
    pub level: u32,
    /// Experience points.
    pub xp: u32,
    /// Full inventory (items + gold).
    pub inventory: Inventory,
    /// Accumulated knowledge from gameplay.
    pub known_facts: Vec<KnownFact>,
    /// Full character serialized as JSON for the dispatch context.
    pub character_json: Option<serde_json::Value>,
}

/// Extract complete character state from a loaded snapshot.
///
/// Returns `None` if the snapshot has no characters — callers must handle
/// this explicitly (no silent fallback to defaults).
pub fn extract_character_state(snapshot: &GameSnapshot) -> Option<RestoredCharacterState> {
    let character = snapshot.characters.first()?;

    let character_json = match serde_json::to_value(character) {
        Ok(json) => Some(json),
        Err(e) => {
            tracing::error!(
                error = %e,
                "session_restore: failed to serialize character to JSON"
            );
            None
        }
    };

    Some(RestoredCharacterState {
        character_name: character.core.name.as_str().to_string(),
        hp: character.core.hp,
        max_hp: character.core.max_hp,
        ac: character.core.ac,
        level: character.core.level,
        xp: character.core.xp,
        inventory: character.core.inventory.clone(),
        known_facts: character.known_facts.clone(),
        character_json,
    })
}
