//! Monster Manual — persistent pre-generated content pool (ADR-059).
//!
//! Server-side GM prep: tool binaries generate NPCs and encounters before the session,
//! results are stored in a persistent JSON file per genre/world. The narrator prompt
//! receives names + brief descriptors via game_state injection. Full stat blocks stay
//! in the Manual for post-narration compound key lookup.
//!
//! The narrator treats game_state as world truth and uses pool names naturally.
//! No XML casting tags, no meta-instructions. World data in the world data section.

use std::path::PathBuf;

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
    /// Location where this NPC was first activated (introduced in narration).
    /// Used to anchor NPCs geographically — they don't follow the player everywhere.
    #[serde(default)]
    pub activated_location: Option<String>,
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
    /// Genre slug this manual belongs to.
    pub genre: String,
    /// World slug this manual belongs to.
    pub world: String,
    /// Pre-generated NPC entries available to this world.
    pub npcs: Vec<ManualNpc>,
    /// Pre-generated encounter entries available to this world.
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
    pub fn mark_active(&mut self, name: &str, location: &str) {
        let name_lower = name.to_lowercase();
        for npc in &mut self.npcs {
            if npc.name.to_lowercase() == name_lower
                || npc.name.to_lowercase().contains(&name_lower)
                || name_lower.contains(&npc.name.to_lowercase())
            {
                npc.state = EntryState::Active;
                if npc.activated_location.is_none() {
                    npc.activated_location = Some(location.to_string());
                }
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
        self.npcs
            .iter()
            .filter(|n| n.state == EntryState::Available)
            .collect()
    }

    /// Available encounters.
    pub fn available_encounters(&self) -> Vec<&ManualEncounter> {
        self.encounters
            .iter()
            .filter(|e| e.state == EntryState::Available)
            .collect()
    }

    /// Whether the Manual needs more Available entries.
    pub fn needs_seeding(&self) -> bool {
        self.available_npcs().len() < 4 || self.available_encounters().is_empty()
    }

    // ── Formatting for game_state injection ─────────────────

    /// Format location-relevant NPCs for injection into the `<game_state>` section.
    ///
    /// Only includes NPCs that are:
    /// - Active at the current location (full profile with personality + speech)
    /// - Available but not yet encountered (name + role only, max 3)
    ///
    /// Dormant NPCs at other locations are omitted entirely — the narrator
    /// doesn't need the full world roster to narrate the current scene.
    pub fn format_nearby_npcs(&self, current_location: &str) -> String {
        let loc_lower = current_location.to_lowercase();

        // Active NPCs at current location → full profiles
        let at_location: Vec<_> = self
            .npcs
            .iter()
            .filter(|n| n.state == EntryState::Active)
            .filter(|n| {
                n.activated_location
                    .as_ref()
                    .map(|l| {
                        l.to_lowercase().contains(&loc_lower)
                            || loc_lower.contains(&l.to_lowercase())
                    })
                    .unwrap_or(true)
            }) // Active with no location → assume present
            .collect();

        // Available NPCs → name-only, capped at 3 so the narrator knows who exists
        let available: Vec<_> = self
            .npcs
            .iter()
            .filter(|n| n.state == EntryState::Available)
            .take(3)
            .collect();

        if at_location.is_empty() && available.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();

        if !at_location.is_empty() {
            lines.push("NPCs present at this location:".to_string());
            for npc in &at_location {
                let ocean_summary = npc
                    .data
                    .get("ocean_summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let quirks: Vec<&str> = npc
                    .data
                    .get("dialogue_quirks")
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
        }

        if !available.is_empty() {
            let names: Vec<String> = available
                .iter()
                .map(|n| format!("{} ({})", n.name, n.role))
                .collect();
            lines.push(format!("Other known NPCs: {}", names.join(", ")));
        }

        lines.join("\n")
    }

    /// Format encounters for injection into `<game_state>`.
    ///
    /// When `in_combat` is true, includes full stat blocks (abilities + weaknesses)
    /// for all available encounters — the narrator needs them for combat resolution.
    ///
    /// When not in combat, includes only name + tier for at most 2 encounters.
    /// The narrator doesn't need 8 creature stat blocks to describe a marketplace.
    pub fn format_area_creatures(&self, in_combat: bool) -> String {
        let available: Vec<_> = self
            .encounters
            .iter()
            .filter(|e| e.state == EntryState::Available)
            .collect();
        if available.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Hostile creatures in the area:".to_string()];
        let limit = if in_combat { available.len() } else { 2 };
        for enc in available.iter().take(limit) {
            if let Some(enemies) = enc.data.get("enemies").and_then(|v| v.as_array()) {
                for enemy in enemies {
                    let name = enemy
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");
                    let class = enemy.get("class").and_then(|v| v.as_str()).unwrap_or("");
                    let tier_label = enemy
                        .get("tier_label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let hp = enemy.get("hp").and_then(|v| v.as_u64()).unwrap_or(0);
                    let role = enemy.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!(
                        "  - {} ({}, {}, HP {}) — {}",
                        name, class, tier_label, hp, role
                    ));
                    // Full stat blocks only during combat
                    if in_combat {
                        let abilities: Vec<&str> = enemy
                            .get("abilities")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(3).collect())
                            .unwrap_or_default();
                        let weaknesses: Vec<&str> = enemy
                            .get("weaknesses")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(2).collect())
                            .unwrap_or_default();
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
        }
        lines.join("\n")
    }

    // ── Insertion ───────────────────────────────────────────

    /// Add a pre-generated NPC from namegen JSON output.
    pub fn add_npc(&mut self, data: serde_json::Value, location_tags: Vec<String>) {
        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let role = data
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let culture = data
            .get("culture")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

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
            activated_location: None,
        });
    }

    /// Add a pre-generated encounter from encountergen JSON output.
    pub fn add_encounter(&mut self, data: serde_json::Value, tier: u32, terrain_tags: Vec<String>) {
        let enemy_names: Vec<String> = data
            .get("enemies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        e.get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
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
        manual.add_npc(
            serde_json::json!({"name": "A", "role": "r", "culture": "c"}),
            vec![],
        );
        manual.add_npc(
            serde_json::json!({"name": "B", "role": "r", "culture": "c"}),
            vec![],
        );

        assert_eq!(manual.available_npcs().len(), 2);

        manual.mark_active("A", "The Collapsed Transit Hub");
        assert_eq!(manual.available_npcs().len(), 1);
        assert_eq!(manual.npcs[0].state, EntryState::Active);
        assert_eq!(
            manual.npcs[0].activated_location.as_deref(),
            Some("The Collapsed Transit Hub")
        );

        manual.mark_all_dormant();
        assert_eq!(manual.npcs[0].state, EntryState::Dormant);
        assert_eq!(manual.npcs[1].state, EntryState::Available); // B was never Active
    }

    #[test]
    fn format_nearby_npcs_filters_by_location() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        manual.add_npc(
            serde_json::json!({
                "name": "Krag Dustwelder",
                "role": "mechanic",
                "culture": "Scrapborn",
                "ocean_summary": "blunt and competitive",
                "dialogue_quirks": ["quotes prices", "mentions danger casually"]
            }),
            vec![],
        );
        manual.add_npc(
            serde_json::json!({
                "name": "Zara Volt",
                "role": "trader",
                "culture": "Vaultborn",
                "ocean_summary": "calm and shrewd"
            }),
            vec![],
        );

        // Mark Krag active at the hub
        manual.mark_active("Krag Dustwelder", "The Hub");

        // At the hub: should see Krag's full profile, Zara as available name-only
        let output = manual.format_nearby_npcs("The Hub");
        assert!(output.contains("NPCs present at this location"));
        assert!(output.contains("Krag Dustwelder"));
        assert!(output.contains("quotes prices")); // full profile for active
        assert!(output.contains("Other known NPCs"));
        assert!(output.contains("Zara Volt"));

        // At a different location: Krag is active elsewhere, not included
        let output2 = manual.format_nearby_npcs("The Market");
        assert!(!output2.contains("Krag Dustwelder"));
        assert!(output2.contains("Zara Volt")); // available name-only
    }

    #[test]
    fn format_area_creatures_combat_vs_exploration() {
        let mut manual = MonsterManual::new("mutant_wasteland", "flickering_reach");
        manual.add_encounter(
            serde_json::json!({
                "enemies": [{
                    "name": "Salt Burrower",
                    "class": "Beastkin",
                    "tier_label": "tier-2",
                    "hp": 14,
                    "role": "ambush predator",
                    "abilities": ["Burrow Ambush", "Mandible Crush"],
                    "weaknesses": ["bright light", "fire"]
                }]
            }),
            2,
            vec![],
        );

        // In combat: full stat blocks
        let output = manual.format_area_creatures(true);
        assert!(output.contains("Hostile creatures"));
        assert!(output.contains("Salt Burrower"));
        assert!(output.contains("Burrow Ambush"));
        assert!(output.contains("bright light"));

        // Not in combat: name + tier only, no abilities/weaknesses
        let output2 = manual.format_area_creatures(false);
        assert!(output2.contains("Salt Burrower"));
        assert!(!output2.contains("Burrow Ambush"));
        assert!(!output2.contains("bright light"));
    }
}
