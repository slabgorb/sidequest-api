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

/// Generate a "Previously On..." recap from known facts and narrative entries.
///
/// Prefers known_facts (concise one-sentence summaries from the knowledge
/// journal) over raw narration. Falls back to narration entries if no facts
/// exist. Returns `None` if both sources are empty.
///
/// Algorithm:
/// 1. Header: "Previously On..."
/// 2. Party intro: "The party — {names} — had been adventuring."
/// 3. Bullets from known_facts (up to 8 most recent, excluding Backstory)
///    OR truncated narration entries as fallback
/// 4. Footer: "The party now finds themselves at {location}."
pub fn generate_recap(
    entries: &[NarrativeEntry],
    character_names: &[String],
    location: &str,
) -> Option<String> {
    generate_recap_with_facts(entries, character_names, location, &[])
}

/// Extended recap that uses known_facts as the primary source.
/// Called from the persistence layer where character facts are available.
pub fn generate_recap_with_facts(
    entries: &[NarrativeEntry],
    character_names: &[String],
    location: &str,
    known_facts: &[crate::known_fact::KnownFact],
) -> Option<String> {
    // Filter to non-backstory facts, take the 8 most recent
    let mut play_facts: Vec<&crate::known_fact::KnownFact> = known_facts
        .iter()
        .filter(|f| !matches!(f.source, crate::known_fact::FactSource::Backstory))
        .collect();
    play_facts.sort_by(|a, b| b.learned_turn.cmp(&a.learned_turn));
    let fact_bullets: Vec<&str> = play_facts
        .iter()
        .take(8)
        .map(|f| f.content.as_str())
        .collect();

    if fact_bullets.is_empty() && entries.is_empty() {
        return None;
    }

    let mut recap = String::from("## Previously On\u{2026}\n\n");

    // Party intro
    if !character_names.is_empty() {
        let names = character_names.join(", ");
        recap.push_str(&format!(
            "The party \u{2014} {} \u{2014} had been adventuring.\n\n",
            names
        ));
    }

    if !fact_bullets.is_empty() {
        // Use known_facts — already concise one-sentence summaries
        for fact in &fact_bullets {
            recap.push_str(&format!("- {}\n", fact));
        }
    } else {
        // Fallback: truncated narration entries (legacy path)
        for entry in entries {
            let content = if entry.content.len() > 200 {
                let truncated = &entry.content[..entry.content.floor_char_boundary(200)];
                format!("{}...", truncated)
            } else {
                entry.content.clone()
            };
            recap.push_str(&format!("- {}\n", content));
        }
    }

    // Location footer
    if !location.is_empty() {
        recap.push_str(&format!(
            "\n*The party now finds themselves at {}.*\n",
            location
        ));
    }

    Some(recap)
}
