//! Monster Manual — persistent pre-generated content pool (ADR-059).
//!
//! Server-side GM prep: tool binaries generate NPCs and encounters before the session,
//! results are stored in a persistent JSON file per genre/world. The narrator prompt
//! receives names + brief descriptors via game_state injection. Full stat blocks stay
//! in the Manual for post-narration compound key lookup.
//!
//! The narrator treats game_state as world truth and uses pool names naturally.
//! No XML casting tags, no meta-instructions. World data in the world data section.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Lifecycle state for a Manual entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    /// Pre-generated, not yet used in narration.
    Available,
    /// Narrator has introduced them, currently in scene.
    Active,
    /// Used previously, not in current scene, can return.
    Dormant,
}

/// A pre-generated NPC identity from sidequest-namegen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualNpc {
    /// Full namegen JSON output (name, OCEAN, personality, dialogue_quirks, etc.).
    pub data: serde_json::Value,
    /// Extracted name for quick reference / compound key.
    pub name: String,
    /// Role (e.g., "wasteland trader", "tech cultist").
    pub role: String,
    /// Culture/faction (e.g., "Scrapborn", "Vaultborn").
    pub culture: String,
    /// Biome/terrain/location tags for future filtering.
    #[serde(default)]
    pub location_tags: Vec<String>,
    /// Lifecycle state.
    pub state: EntryState,
}

/// A pre-generated encounter block from sidequest-encountergen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualEncounter {
    /// Full encountergen JSON output (enemies array with stats, abilities, etc.).
    pub data: serde_json::Value,
    /// Summary label (e.g., "2x Salt Burrower (tier 2)").
    pub label: String,
    /// Power tier (1-4).
    pub tier: u32,
    /// Biome/terrain tags for future filtering.
    #[serde(default)]
    pub terrain_tags: Vec<String>,
    /// Lifecycle state.
    pub state: EntryState,
}

/// Persistent Monster Manual for a genre/world combination.
///
/// Stored as JSON at `~/.sidequest/manuals/{genre}_{world}.json`.
/// Grows over play sessions — every generated entry persists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterManual {
    pub genre: String,
    pub world: String,
    pub npcs: Vec<ManualNpc>,
    pub encounters: Vec<ManualEncounter>,
}

impl MonsterManual {
    /// Create an empty Manual for a genre/world.
    pub fn new(genre: &str, world: &str) -> Self {
        Self {
            genre: genre.to_string(),
            world: world.to_string(),
            npcs: Vec::new(),
            encounters: Vec::new(),
        }
    }

    /// Directory where Manual files are stored.
    fn manuals_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".sidequest")
            .join("manuals")
    }

    /// File path for this genre/world Manual.
    fn file_path(genre: &str, world: &str) -> PathBuf {
        Self::manuals_dir().join(format!("{}_{}.json", genre, world))
    }

    /// Load a Manual from disk. Returns a new empty Manual if file doesn't exist.
    pub fn load(genre: &str, world: &str) -> Self {
        let path = Self::file_path(genre, world);
        if !path.exists() {
            return Self::new(genre, world);
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!(error = %e, path = %path.display(), "monster_manual.load_failed — starting fresh");
                Self::new(genre, world)
            }),
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "monster_manual.read_failed — starting fresh");
                Self::new(genre, world)
            }
        }
    }

    /// Save this Manual to disk.
    pub fn save(&self) {
        let dir = Self::manuals_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, "monster_manual.mkdir_failed");
            return;
        }
        let path = Self::file_path(&self.genre, &self.world);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!(error = %e, path = %path.display(), "monster_manual.save_failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "monster_manual.serialize_failed");
            }
        }
    }

    // ── Lookup ──────────────────────────────────────────────

    /// Compound key lookup: find an NPC by (name, culture, world).
    pub fn get_npc(&self, name: &str, culture: &str) -> Option<&ManualNpc> {
        let name_lower = name.to_lowercase();
        let culture_lower = culture.to_lowercase();
        self.npcs.iter().find(|n| {
            n.name.to_lowercase() == name_lower && n.culture.to_lowercase() == culture_lower
        })
    }

    /// Find an NPC by name alone (fuzzy — substring match).
    pub fn find_npc_by_name(&self, name: &str) -> Option<&ManualNpc> {
        let name_lower = name.to_lowercase();
        self.npcs.iter().find(|n| {
            n.name.to_lowercase() == name_lower
                || n.name.to_lowercase().contains(&name_lower)
                || name_lower.contains(&n.name.to_lowercase())
        })
    }

    // ── Lifecycle ───────────────────────────────────────────

    /// Mark an NPC as Active by name (case-insensitive).
    pub fn mark_active(&mut self, name: &str) {
        let name_lower = name.to_lowercase();
        for npc in &mut self.npcs {
            if npc.name.to_lowercase() == name_lower
                || npc.name.to_lowercase().contains(&name_lower)
                || name_lower.contains(&npc.name.to_lowercase())
            {
                npc.state = EntryState::Active;
                return;
            }
        }
    }

    /// Transition all Active entries to Dormant (call on location change).
    pub fn mark_all_dormant(&mut self) {
        for npc in &mut self.npcs {
            if npc.state == EntryState::Active {
                npc.state = EntryState::Dormant;
            }
        }
        for enc in &mut self.encounters {
            if enc.state == EntryState::Active {
                enc.state = EntryState::Dormant;
            }
        }
    }

    /// Available NPCs (not yet used in narration).
    pub fn available_npcs(&self) -> Vec<&ManualNpc> {
        self.npcs.iter().filter(|n| n.state == EntryState::Available).collect()
    }

    /// Available encounters.
    pub fn available_encounters(&self) -> Vec<&ManualEncounter> {
        self.encounters.iter().filter(|e| e.state == EntryState::Available).collect()
    }

    /// Whether the Manual needs more Available entries.
    pub fn needs_seeding(&self) -> bool {
        self.available_npcs().len() < 2 || self.available_encounters().is_empty()
    }

    // ── Formatting for game_state injection ─────────────────

    /// Format Available + Active NPCs for injection into the `<game_state>` section.
    ///
    /// Output looks like:
    /// ```text
    /// NPCs nearby (not yet met by player):
    ///   - Joch Glowvein (wasteland trader, Scrapborn) — blunt, quotes prices in three barter systems
    /// ```
    pub fn format_nearby_npcs(&self) -> String {
        let available: Vec<_> = self.npcs.iter()
            .filter(|n| n.state == EntryState::Available)
            .collect();
        if available.is_empty() {
            return String::new();
        }

        let mut lines = vec!["NPCs nearby (not yet met by player):".to_string()];
        for npc in &available {
            let ocean_summary = npc.data.get("ocean_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let quirks: Vec<&str> = npc.data.get("dialogue_quirks")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(2).collect())
                .unwrap_or_default();
            let quirk_str = if quirks.is_empty() {
                String::new()
            } else {
                format!("\n    Speech: {}", quirks.join("; "))
            };
            lines.push(format!(
                "  - {} ({}, {}) — {}{}",
                npc.name, npc.role, npc.culture, ocean_summary, quirk_str
            ));
        }
        lines.join("\n")
    }

    /// Format Available encounters for injection into `<game_state>`.
    ///
    /// Output looks like:
    /// ```text
    /// Hostile creatures in the area:
    ///   - Salt Burrower (tier 2, HP 14) — eyeless ambush predator
    ///     Abilities: Burrow Ambush, Mandible Crush. Weakness: bright light, fire.
    /// ```
    pub fn format_area_creatures(&self) -> String {
        let available: Vec<_> = self.encounters.iter()
            .filter(|e| e.state == EntryState::Available)
            .collect();
        if available.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Hostile creatures in the area:".to_string()];
        for enc in &available {
            if let Some(enemies) = enc.data.get("enemies").and_then(|v| v.as_array()) {
                for enemy in enemies {
                    let name = enemy.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                    let class = enemy.get("class").and_then(|v| v.as_str()).unwrap_or("");
                    let tier_label = enemy.get("tier_label").and_then(|v| v.as_str()).unwrap_or("?");
                    let hp = enemy.get("hp").and_then(|v| v.as_u64()).unwrap_or(0);
                    let role = enemy.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let abilities: Vec<&str> = enemy.get("abilities")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(3).collect())
                        .unwrap_or_default();
                    let weaknesses: Vec<&str> = enemy.get("weaknesses")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(2).collect())
                        .unwrap_or_default();
                    lines.push(format!(
                        "  - {} ({}, {}, HP {}) — {}",
                        name, class, tier_label, hp, role
                    ));
                    if !abilities.is_empty() || !weaknesses.is_empty() {
                        lines.push(format!(
                            "    Abilities: {}. Weakness: {}.",
                            abilities.join(", "),
                            weaknesses.join(", ")
                        ));
                    }
                }
            }
        }
        lines.join("\n")
    }

    // ── Insertion ───────────────────────────────────────────

    /// Add a pre-generated NPC from namegen JSON output.
    pub fn add_npc(&mut self, data: serde_json::Value, location_tags: Vec<String>) {
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let role = data.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let culture = data.get("culture").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Don't add duplicates
        if self.find_npc_by_name(&name).is_some() {
            return;
        }

        self.npcs.push(ManualNpc {
            data,
            name,
            role,
            culture,
            location_tags,
            state: EntryState::Available,
        });
    }

    /// Add a pre-generated encounter from encountergen JSON output.
    pub fn add_encounter(&mut self, data: serde_json::Value, tier: u32, terrain_tags: Vec<String>) {
        let enemy_names: Vec<String> = data.get("enemies")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|e| e.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let label = if enemy_names.is_empty() {
            format!("encounter (tier {})", tier)
        } else {
            format!("{} (tier {})", enemy_names.join(", "), tier)
        };

        self.encounters.push(ManualEncounter {
            data,
            label,
            tier,
            terrain_tags,
            state: EntryState::Available,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manual_is_empty() {
        let manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        assert!(manual.npcs.is_empty());
        assert!(manual.encounters.is_empty());
        assert!(manual.needs_seeding());
    }

    #[test]
    fn add_npc_and_lookup() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        let data = serde_json::json!({
            "name": "Krag Dustwelder",
            "role": "mechanic",
            "culture": "Scrapborn",
            "ocean_summary": "blunt and competitive",
            "dialogue_quirks": ["quotes prices in three barter systems"]
        });
        manual.add_npc(data, vec![]);

        assert_eq!(manual.npcs.len(), 1);
        assert!(manual.get_npc("Krag Dustwelder", "Scrapborn").is_some());
        assert!(manual.get_npc("krag dustwelder", "scrapborn").is_some()); // case-insensitive
        assert!(manual.find_npc_by_name("Krag").is_some()); // substring
    }

    #[test]
    fn dedup_prevents_double_add() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        let data = serde_json::json!({"name": "Krag", "role": "mechanic", "culture": "Scrapborn"});
        manual.add_npc(data.clone(), vec![]);
        manual.add_npc(data, vec![]);
        assert_eq!(manual.npcs.len(), 1);
    }

    #[test]
    fn lifecycle_transitions() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        manual.add_npc(serde_json::json!({"name": "A", "role": "r", "culture": "c"}), vec![]);
        manual.add_npc(serde_json::json!({"name": "B", "role": "r", "culture": "c"}), vec![]);

        assert_eq!(manual.available_npcs().len(), 2);

        manual.mark_active("A");
        assert_eq!(manual.available_npcs().len(), 1);
        assert_eq!(manual.npcs[0].state, EntryState::Active);

        manual.mark_all_dormant();
        assert_eq!(manual.npcs[0].state, EntryState::Dormant);
        assert_eq!(manual.npcs[1].state, EntryState::Available); // B was never Active
    }

    #[test]
    fn format_nearby_npcs_output() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        manual.add_npc(serde_json::json!({
            "name": "Krag Dustwelder",
            "role": "mechanic",
            "culture": "Scrapborn",
            "ocean_summary": "blunt and competitive",
            "dialogue_quirks": ["quotes prices", "mentions danger casually"]
        }), vec![]);

        let output = manual.format_nearby_npcs();
        assert!(output.contains("NPCs nearby"));
        assert!(output.contains("Krag Dustwelder"));
        assert!(output.contains("mechanic"));
        assert!(output.contains("quotes prices"));
    }

    #[test]
    fn format_area_creatures_output() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        manual.add_encounter(serde_json::json!({
            "enemies": [{
                "name": "Salt Burrower",
                "class": "Beastkin",
                "tier_label": "tier-2",
                "hp": 14,
                "role": "ambush predator",
                "abilities": ["Burrow Ambush", "Mandible Crush"],
                "weaknesses": ["bright light", "fire"]
            }]
        }), 2, vec![]);

        let output = manual.format_area_creatures();
        assert!(output.contains("Hostile creatures"));
        assert!(output.contains("Salt Burrower"));
        assert!(output.contains("Burrow Ambush"));
        assert!(output.contains("bright light"));
    }
}
