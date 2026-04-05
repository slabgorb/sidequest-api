//! Narrative tracking — append-only, immutable entries.
//!
//! NarrativeEntry is a simple data struct. The narrative log is a Vec
//! of entries, queried via reverse iteration (newest first).
//! No edits or deletes — append only.
//!
//! Story F3: EncounterTag for NPC encounter tracking, generate_recap() for
//! "Previously On..." session resume.

use serde::{Deserialize, Serialize};

/// Tag recording an NPC encounter within a narrative entry (story F3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterTag {
    /// NPC identifier.
    pub npc_id: String,
    /// Type of encounter (e.g., "combat", "dialogue", "trade").
    pub encounter_type: String,
    /// Optional archetype identifier.
    #[serde(default)]
    pub archetype_id: Option<String>,
    /// Optional notes about the encounter.
    #[serde(default)]
    pub notes: Option<String>,
}

/// A single narrative entry in the game log.
///
/// Entries are immutable once created. The narrative log is a `Vec<NarrativeEntry>`
/// that only grows via `push`. Query via `.iter().rev()` for newest-first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeEntry {
    /// Milliseconds since game start.
    pub timestamp: u64,
    /// Which game round this occurred in.
    pub round: u32,
    /// Source of the narration (e.g., "narrator", "combat", "chase").
    pub author: String,
    /// The narration text.
    pub content: String,
    /// Tags for scene filtering.
    pub tags: Vec<String>,
    /// NPC encounter tags (story F3).
    #[serde(default)]
    pub encounter_tags: Vec<EncounterTag>,
    /// Speaking character, if this is dialogue.
    #[serde(default)]
    pub speaker: Option<String>,
    /// Entry type classification (e.g., "combat", "dialogue", "event").
    #[serde(default)]
    pub entry_type: Option<String>,
}

/// Generate a "Previously On..." recap from narrative entries (story F3).
///
/// Returns `None` if `entries` is empty.
///
/// Algorithm:
/// 1. Header: "Previously On..."
/// 2. Party intro: "The party — {names} — had been adventuring."
/// 3. Each entry as a bullet: "- {content}"
/// 4. Footer: "The party now finds themselves at {location}."
pub fn generate_recap(
    entries: &[NarrativeEntry],
    character_names: &[String],
    location: &str,
) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut recap = String::from("Previously On...\n\n");

    // Party intro
    if !character_names.is_empty() {
        let names = character_names.join(", ");
        recap.push_str(&format!(
            "The party \u{2014} {} \u{2014} had been adventuring.\n\n",
            names
        ));
    }

    // Entry bullets — truncate long entries to keep recap concise
    for entry in entries {
        let content = if entry.content.len() > 200 {
            let truncated = &entry.content[..entry.content.floor_char_boundary(200)];
            format!("{}...", truncated)
        } else {
            entry.content.clone()
        };
        recap.push_str(&format!("- {}\n", content));
    }

    // Location footer
    if !location.is_empty() {
        recap.push_str(&format!(
            "\nThe party now finds themselves at {}.\n",
            location
        ));
    }

    Some(recap)
}
