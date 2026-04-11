//! Campaign maturity, world materialization, and WorldBuilder (Stories 6-6, 18-8).
//!
//! Determines campaign maturity from turn count and story beats fired,
//! then bootstraps the GameSnapshot with appropriate history chapters
//! from the genre pack.
//!
//! Story 18-8 adds the WorldBuilder fluent API ported from Python's
//! `sidequest/game/world_builder.py`. The builder materializes dense
//! GameState at any maturity level for playtesting.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::belief_state::BeliefState;
use crate::character::Character;
use crate::creature_core::CreatureCore;
use crate::disposition::Disposition;
use crate::inventory::Inventory;
use crate::narrative::NarrativeEntry;
use crate::npc::Npc;
use crate::state::GameSnapshot;
use crate::trope::{TropeState, TropeStatus};

// ═══════════════════════════════════════════════════════════════
// CampaignMaturity (Story 6-6 — unchanged)
// ═══════════════════════════════════════════════════════════════

/// Campaign maturity level derived from game progression.
///
/// Maturity controls which history chapters are applied to the GameSnapshot,
/// letting fresh campaigns feel sparse and veteran campaigns feel rich.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CampaignMaturity {
    /// Turns 0-5 effective: minimal history, world is new.
    Fresh,
    /// Turns 6-20 effective: factions introduced, stakes emerging.
    Early,
    /// Turns 21-50 effective: established relationships, escalating tensions.
    Mid,
    /// Turns 51+ effective: deep history, faction conflicts in motion.
    Veteran,
}

impl Default for CampaignMaturity {
    fn default() -> Self {
        Self::Fresh
    }
}

impl CampaignMaturity {
    /// Derive maturity from a game snapshot's turn count and beats fired.
    ///
    /// Beats accelerate maturity — a dramatic early game matures faster.
    /// Uses saturating arithmetic to prevent overflow with large beat counts.
    pub fn from_snapshot(snapshot: &GameSnapshot) -> Self {
        let turn = snapshot.turn_manager.round();
        let beats = snapshot.total_beats_fired;
        let effective_turns = (turn as u64).saturating_add((beats / 2) as u64);
        match effective_turns {
            0..=5 => Self::Fresh,
            6..=20 => Self::Early,
            21..=50 => Self::Mid,
            _ => Self::Veteran,
        }
    }

    /// Map a chapter id string to the corresponding maturity level.
    fn from_chapter_id(id: &str) -> Option<Self> {
        match id {
            "fresh" => Some(Self::Fresh),
            "early" => Some(Self::Early),
            "mid" => Some(Self::Mid),
            "veteran" => Some(Self::Veteran),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Chapter sub-types (Story 18-8)
// ═══════════════════════════════════════════════════════════════

/// Character data within a history chapter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChapterCharacter {
    /// Character name.
    #[serde(default)]
    pub name: String,
    /// Character race.
    #[serde(default)]
    pub race: String,
    /// Character class.
    #[serde(default, alias = "class")]
    pub class: String,
    /// Character level.
    #[serde(default)]
    pub level: u32,
    /// Current HP.
    #[serde(default)]
    pub hp: Option<i32>,
    /// Maximum HP.
    #[serde(default)]
    pub max_hp: Option<i32>,
    /// Armor class.
    #[serde(default)]
    pub ac: Option<i32>,
    /// Backstory text.
    #[serde(default)]
    pub backstory: Option<String>,
    /// Personality description.
    #[serde(default)]
    pub personality: Option<String>,
    /// Physical description.
    #[serde(default)]
    pub description: Option<String>,
    /// Gold amount.
    #[serde(default)]
    pub gold: Option<i32>,
}

/// NPC data within a history chapter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChapterNpc {
    /// NPC name.
    #[serde(default)]
    pub name: String,
    /// NPC role identifier.
    #[serde(default)]
    pub role: Option<String>,
    /// Physical description.
    #[serde(default)]
    pub description: Option<String>,
    /// Personality description.
    #[serde(default)]
    pub personality: Option<String>,
    /// Numeric disposition value.
    #[serde(default)]
    pub disposition: Option<i32>,
    /// Current location.
    #[serde(default)]
    pub location: Option<String>,
    /// Backstory text.
    #[serde(default)]
    pub backstory: Option<String>,
    /// Archetype name for instantiation.
    #[serde(default)]
    pub archetype: Option<String>,
    /// Dialogue quirks.
    #[serde(default)]
    pub dialogue_quirks: Vec<String>,
}

/// A narrative log entry within a history chapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterNarrativeEntry {
    /// Speaker (e.g., "narrator", character name).
    pub speaker: String,
    /// Narration text.
    pub text: String,
}

/// Trope state within a history chapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterTrope {
    /// Trope definition ID.
    pub id: String,
    /// Status string (active, dormant, progressing, resolved).
    pub status: String,
    /// Progression value (0.0 to 1.0).
    #[serde(default)]
    pub progression: f64,
    /// Notes about the trope state.
    #[serde(default)]
    pub notes: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════
// HistoryChapter (expanded for Story 18-8)
// ═══════════════════════════════════════════════════════════════

/// A history chapter from the genre pack, keyed by maturity level.
///
/// Story 18-8 expanded this from the minimal 6-6 version (id, label, lore)
/// to carry full chapter data matching the Python WorldBuilder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryChapter {
    /// Maturity level key (fresh, early, mid, veteran).
    pub id: String,
    /// Human-readable chapter title.
    pub label: String,
    /// Lore fragments for this chapter.
    #[serde(default)]
    pub lore: Vec<String>,
    /// Session range (for genre pack loader — not used by WorldBuilder directly).
    #[serde(default)]
    pub session_range: Option<Vec<u32>>,
    /// Player character data for this chapter.
    #[serde(default)]
    pub character: Option<ChapterCharacter>,
    /// NPCs introduced or updated in this chapter.
    #[serde(default)]
    pub npcs: Vec<ChapterNpc>,
    /// Quest log entries (quest_name → status).
    #[serde(default)]
    pub quests: HashMap<String, String>,
    /// Player notes.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Narrative log entries.
    #[serde(default)]
    pub narrative_log: Vec<ChapterNarrativeEntry>,
    /// Current location name.
    #[serde(default)]
    pub location: Option<String>,
    /// Time of day.
    #[serde(default)]
    pub time_of_day: Option<String>,
    /// Atmosphere description.
    #[serde(default)]
    pub atmosphere: Option<String>,
    /// Active stakes description.
    #[serde(default)]
    pub active_stakes: Option<String>,
    /// Points of interest (stored as raw Value for forward-compatibility).
    #[serde(default)]
    pub points_of_interest: Option<serde_json::Value>,
    /// Trope states for this chapter.
    #[serde(default)]
    pub tropes: Vec<ChapterTrope>,
}

// ═══════════════════════════════════════════════════════════════
// materialize_world (Story 6-6 — unchanged)
// ═══════════════════════════════════════════════════════════════

/// Apply history chapters to a GameSnapshot based on campaign maturity.
///
/// Calculates maturity from the snapshot, then includes all chapters whose
/// maturity level is at or below the current level. Idempotent — replaces
/// existing world_history and campaign_maturity on each call.
pub fn materialize_world(snapshot: &mut GameSnapshot, chapters: &[HistoryChapter]) {
    let maturity = CampaignMaturity::from_snapshot(snapshot);
    let applicable: Vec<HistoryChapter> = chapters
        .iter()
        .filter(|ch| {
            CampaignMaturity::from_chapter_id(&ch.id)
                .map(|ch_maturity| ch_maturity <= maturity)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    snapshot.world_history = applicable;
    snapshot.campaign_maturity = maturity;
}

// ═══════════════════════════════════════════════════════════════
// WorldBuilder (Story 18-8)
// ═══════════════════════════════════════════════════════════════

/// Fluent builder that produces a GameSnapshot at a given campaign maturity.
///
/// Ported from Python's `sidequest/game/world_builder.py`. The builder
/// accepts chapters and configuration, then materializes a dense GameSnapshot
/// with characters, NPCs, quests, lore, narrative log, scene context,
/// tropes, and optional extras (extra NPCs, extra lore, combat setup).
pub struct WorldBuilder {
    maturity: CampaignMaturity,
    chapters: Vec<HistoryChapter>,
    extra_npcs: usize,
    extra_lore: usize,
    /// None = no combat, Some(None) = default enemies, Some(Some(list)) = custom enemies.
    combat_enemies: Option<Option<Vec<(String, i32, i32)>>>,
}

impl WorldBuilder {
    /// Create a new WorldBuilder with Fresh maturity and no chapters.
    pub fn new() -> Self {
        Self {
            maturity: CampaignMaturity::Fresh,
            chapters: Vec::new(),
            extra_npcs: 0,
            extra_lore: 0,
            combat_enemies: None,
        }
    }

    /// Set the target campaign maturity level.
    pub fn at_maturity(mut self, maturity: CampaignMaturity) -> Self {
        self.maturity = maturity;
        self
    }

    /// Provide history chapters to apply.
    pub fn with_chapters(mut self, chapters: Vec<HistoryChapter>) -> Self {
        self.chapters = chapters;
        self
    }

    /// Add generated extra NPCs for stress testing.
    pub fn with_extra_npcs(mut self, count: usize) -> Self {
        self.extra_npcs = count;
        self
    }

    /// Add generated extra lore entries.
    pub fn with_extra_lore(mut self, count: usize) -> Self {
        self.extra_lore = count;
        self
    }

    /// Set up combat with optional custom enemies.
    /// None = default enemies, Some(list) = custom enemies as (name, hp, max_hp).
    pub fn with_combat(mut self, enemies: Option<Vec<(String, i32, i32)>>) -> Self {
        self.combat_enemies = Some(enemies);
        self
    }

    /// Build a GameSnapshot at the configured maturity level.
    ///
    /// Filters chapters by maturity (cumulative), applies each chapter's data
    /// to the snapshot, then applies extras (NPCs, lore, combat).
    pub fn build(&self) -> GameSnapshot {
        let mut snap = GameSnapshot::default();
        snap.campaign_maturity = self.maturity.clone();

        // Filter chapters by maturity (cumulative — include all at or below target)
        let applicable: Vec<&HistoryChapter> = self
            .chapters
            .iter()
            .filter(|ch| {
                CampaignMaturity::from_chapter_id(&ch.id)
                    .map(|ch_maturity| ch_maturity <= self.maturity)
                    .unwrap_or(false)
            })
            .collect();

        // Apply each chapter cumulatively
        for chapter in &applicable {
            self.apply_chapter(&mut snap, chapter);
        }

        // Store applicable chapters as world_history
        snap.world_history = applicable.into_iter().cloned().collect();

        // Apply extras
        if self.extra_npcs > 0 {
            self.add_extra_npcs(&mut snap);
        }

        if self.extra_lore > 0 {
            self.add_extra_lore(&mut snap);
        }

        if let Some(ref enemies) = self.combat_enemies {
            self.setup_combat(&mut snap, enemies.as_ref());
        }

        snap
    }

    /// Apply a single history chapter to the game snapshot (cumulative).
    fn apply_chapter(&self, snap: &mut GameSnapshot, chapter: &HistoryChapter) {
        // Character
        if let Some(ref char_data) = chapter.character {
            self.apply_character(snap, char_data);
        }

        // NPCs
        for npc_data in &chapter.npcs {
            self.apply_npc(snap, npc_data);
        }

        // Quests (insert/update)
        for (quest_name, status) in &chapter.quests {
            snap.quest_log.insert(quest_name.clone(), status.clone());
        }

        // Lore (append, deduplicated)
        for entry in &chapter.lore {
            if !snap.lore_established.contains(entry) {
                snap.lore_established.push(entry.clone());
            }
        }

        // Notes (append)
        snap.notes.extend(chapter.notes.iter().cloned());

        // Narrative log (append, converting to NarrativeEntry)
        for entry in &chapter.narrative_log {
            snap.narrative_log.push(NarrativeEntry {
                timestamp: 0,
                round: 0,
                author: entry.speaker.clone(),
                content: entry.text.clone(),
                tags: Vec::new(),
                encounter_tags: Vec::new(),
                speaker: Some(entry.speaker.clone()),
                entry_type: None,
            });
        }

        // Scene context (overwrite from latest chapter)
        if let Some(ref loc) = chapter.location {
            snap.location = loc.clone();
        }
        if let Some(ref tod) = chapter.time_of_day {
            snap.time_of_day = tod.clone();
        }
        if let Some(ref atm) = chapter.atmosphere {
            snap.atmosphere = atm.clone();
        }
        if let Some(ref stakes) = chapter.active_stakes {
            snap.active_stakes = stakes.clone();
        }

        // Tropes
        for trope_data in &chapter.tropes {
            self.apply_trope(snap, trope_data);
        }
    }

    /// Build or update the player character from chapter data.
    fn apply_character(&self, snap: &mut GameSnapshot, char_data: &ChapterCharacter) {
        if snap.characters.is_empty() {
            // Create character — use provided values or sensible defaults
            let name = if char_data.name.is_empty() {
                "Adventurer"
            } else {
                &char_data.name
            };
            let race = if char_data.race.is_empty() {
                "Human"
            } else {
                &char_data.race
            };
            let class = if char_data.class.is_empty() {
                "Fighter"
            } else {
                &char_data.class
            };
            let description = char_data.description.as_deref().unwrap_or("An adventurer.");
            let personality = char_data.personality.as_deref().unwrap_or("Determined.");
            let backstory = char_data.backstory.as_deref().unwrap_or("");

            let core = CreatureCore {
                name: NonBlankString::new(name)
                    .unwrap_or_else(|_| NonBlankString::new("Adventurer").unwrap()),
                description: NonBlankString::new(description)
                    .unwrap_or_else(|_| NonBlankString::new("An adventurer.").unwrap()),
                personality: NonBlankString::new(personality)
                    .unwrap_or_else(|_| NonBlankString::new("Determined.").unwrap()),
                level: char_data.level,
                hp: char_data.hp.unwrap_or(20),
                max_hp: char_data.max_hp.unwrap_or(20),
                ac: char_data.ac.unwrap_or(10),
                xp: 0,
                inventory: Inventory::default(),
                statuses: Vec::new(),
            };

            let character = Character {
                core,
                backstory: NonBlankString::new(if backstory.is_empty() {
                    "Unknown origins."
                } else {
                    backstory
                })
                .unwrap_or_else(|_| NonBlankString::new("Unknown origins.").unwrap()),
                narrative_state: String::new(),
                hooks: Vec::new(),
                char_class: NonBlankString::new(class)
                    .unwrap_or_else(|_| NonBlankString::new("Fighter").unwrap()),
                race: NonBlankString::new(race)
                    .unwrap_or_else(|_| NonBlankString::new("Human").unwrap()),
                pronouns: String::new(),
                stats: HashMap::new(),
                abilities: Vec::new(),
                affinities: Vec::new(),
                is_friendly: true,
                known_facts: Vec::new(),
            };
            snap.characters.push(character);
        } else {
            // Update existing character
            let char = &mut snap.characters[0];
            if char_data.level > 0 {
                char.core.level = char_data.level;
            }
            if let Some(hp) = char_data.hp {
                char.core.hp = hp;
            }
            if let Some(max_hp) = char_data.max_hp {
                char.core.max_hp = max_hp;
            }
            if let Some(ac) = char_data.ac {
                char.core.ac = ac;
            }
            if !char_data.name.is_empty() {
                if let Ok(name) = NonBlankString::new(&char_data.name) {
                    char.core.name = name;
                }
            }
            if !char_data.race.is_empty() {
                if let Ok(race) = NonBlankString::new(&char_data.race) {
                    char.race = race;
                }
            }
            if !char_data.class.is_empty() {
                if let Ok(class) = NonBlankString::new(&char_data.class) {
                    char.char_class = class;
                }
            }
            if let Some(ref backstory) = char_data.backstory {
                if let Ok(bs) = NonBlankString::new(backstory) {
                    char.backstory = bs;
                }
            }
            if let Some(ref personality) = char_data.personality {
                if let Ok(p) = NonBlankString::new(personality) {
                    char.core.personality = p;
                }
            }
            if let Some(ref description) = char_data.description {
                if let Ok(d) = NonBlankString::new(description) {
                    char.core.description = d;
                }
            }
        }
    }

    /// Instantiate or update an NPC from chapter data.
    fn apply_npc(&self, snap: &mut GameSnapshot, npc_data: &ChapterNpc) {
        if npc_data.name.is_empty() {
            return;
        }

        // Check for existing NPC by name
        let existing = snap
            .npcs
            .iter_mut()
            .find(|n| n.core.name.as_str() == npc_data.name);

        if let Some(npc) = existing {
            // Update existing NPC
            if let Some(disp) = npc_data.disposition {
                npc.disposition = Disposition::new(disp);
            }
            if let Some(ref desc) = npc_data.description {
                if let Ok(d) = NonBlankString::new(desc) {
                    npc.core.description = d;
                }
            }
            if let Some(ref loc) = npc_data.location {
                if let Ok(l) = NonBlankString::new(loc) {
                    npc.location = Some(l);
                }
            }
            if let Some(ref personality) = npc_data.personality {
                if let Ok(p) = NonBlankString::new(personality) {
                    npc.core.personality = p;
                }
            }
            return;
        }

        // Create new NPC
        let name = NonBlankString::new(&npc_data.name)
            .unwrap_or_else(|_| NonBlankString::new("Unknown NPC").unwrap());
        let description = npc_data
            .description
            .as_deref()
            .and_then(|d| NonBlankString::new(d).ok())
            .unwrap_or_else(|| NonBlankString::new("An NPC.").unwrap());
        let personality = npc_data
            .personality
            .as_deref()
            .and_then(|p| NonBlankString::new(p).ok())
            .unwrap_or_else(|| NonBlankString::new("Neutral.").unwrap());

        let core = CreatureCore {
            name,
            description,
            personality,
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            inventory: Inventory::default(),
            statuses: Vec::new(),
        };

        let npc = Npc {
            core,
            voice_id: None,
            disposition: Disposition::new(npc_data.disposition.unwrap_or(0)),
            location: npc_data
                .location
                .as_deref()
                .and_then(|l| NonBlankString::new(l).ok()),
            pronouns: None,
            appearance: None,
            age: None,
            build: None,
            height: None,
            distinguishing_features: Vec::new(),
            ocean: None,
            belief_state: BeliefState::default(),
        };
        snap.npcs.push(npc);
    }

    /// Set trope state from chapter data.
    fn apply_trope(&self, snap: &mut GameSnapshot, trope_data: &ChapterTrope) {
        if trope_data.id.is_empty() {
            return;
        }

        let status = match trope_data.status.as_str() {
            "dormant" => TropeStatus::Dormant,
            "active" => TropeStatus::Active,
            "progressing" => TropeStatus::Progressing,
            "resolved" => TropeStatus::Resolved,
            _ => TropeStatus::Active,
        };

        // Find existing trope
        let existing = snap
            .active_tropes
            .iter_mut()
            .find(|t| t.trope_definition_id() == trope_data.id);

        let trope = if let Some(trope) = existing {
            trope
        } else {
            snap.active_tropes.push(TropeState::new(&trope_data.id));
            snap.active_tropes.last_mut().unwrap()
        };
        trope.set_status(status);
        trope.set_progression(trope_data.progression);
        for note in &trope_data.notes {
            trope.add_note(note.clone());
        }
    }

    /// Generate extra NPCs for stress testing.
    fn add_extra_npcs(&self, snap: &mut GameSnapshot) {
        for i in 0..self.extra_npcs {
            let name = format!("Extra NPC #{}", i + 1);
            let core = CreatureCore {
                name: NonBlankString::new(&name).unwrap(),
                description: NonBlankString::new(&format!(
                    "Generated NPC for stress testing (#{})",
                    i + 1
                ))
                .unwrap(),
                personality: NonBlankString::new("Neutral.").unwrap(),
                level: 1,
                hp: 10,
                max_hp: 10,
                ac: 10,
                xp: 0,
                inventory: Inventory::default(),
                statuses: Vec::new(),
            };
            let npc = Npc {
                core,
                voice_id: None,
                disposition: Disposition::new(0),
                location: None,
                pronouns: None,
                appearance: None,
                age: None,
                build: None,
                height: None,
                distinguishing_features: Vec::new(),
                ocean: None,
                belief_state: BeliefState::default(),
            };
            snap.npcs.push(npc);
        }
    }

    /// Add generated extra lore entries (deduplicated).
    fn add_extra_lore(&self, snap: &mut GameSnapshot) {
        for i in 0..self.extra_lore {
            let entry = format!("Generated lore fact #{}", i + 1);
            if !snap.lore_established.contains(&entry) {
                snap.lore_established.push(entry);
            }
        }
    }

    /// Set up combat encounter with optional custom enemies.
    /// Story 28-9: Uses StructuredEncounter instead of deleted CombatState.
    fn setup_combat(&self, snap: &mut GameSnapshot, enemies: Option<&Vec<(String, i32, i32)>>) {
        use crate::encounter::{
            EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
        };

        let enemy_list: Vec<(String, i32, i32)> = match enemies {
            Some(list) => list.clone(),
            None => vec![("Bandit".to_string(), 12, 12)],
        };

        // Build actors: characters first, then enemies
        let mut actors: Vec<EncounterActor> = snap
            .characters
            .iter()
            .map(|c| EncounterActor {
                name: c.core.name.as_str().to_string(),
                role: "player".to_string(),
            })
            .collect();
        actors.extend(enemy_list.iter().map(|(name, _, _)| EncounterActor {
            name: name.clone(),
            role: "combatant".to_string(),
        }));

        snap.encounter = Some(StructuredEncounter {
            encounter_type: "combat".to_string(),
            metric: EncounterMetric {
                name: "morale".to_string(),
                current: 100,
                starting: 100,
                direction: MetricDirection::Descending,
                threshold_high: None,
                threshold_low: Some(0),
            },
            beat: 0,
            structured_phase: Some(crate::encounter::EncounterPhase::Setup),
            secondary_stats: None,
            actors,
            outcome: None,
            resolved: false,
            mood_override: Some("combat".to_string()),
            narrator_hints: vec![],
        });
    }
}

impl Default for WorldBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Genre pack integration (Story 15-23)
// ═══════════════════════════════════════════════════════════════

/// Parse history chapters from raw genre pack JSON (history.yaml loaded as Value).
///
/// The genre pack loader stores history.yaml as `Option<serde_json::Value>`.
/// This function extracts the `"chapters"` array and deserializes each entry
/// into a typed `HistoryChapter`.
///
/// Returns an empty Vec for null/missing data, errors on malformed chapters.
pub fn parse_history_chapters(value: &serde_json::Value) -> Result<Vec<HistoryChapter>, String> {
    if value.is_null() {
        return Ok(Vec::new());
    }

    let chapters_value = match value.get("chapters") {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };

    serde_json::from_value::<Vec<HistoryChapter>>(chapters_value.clone())
        .map_err(|e| format!("failed to parse history chapters: {e}"))
}

/// Materialize a GameSnapshot from raw genre pack history at a target maturity.
///
/// This is the integration function the server calls during session creation.
/// Parses history chapters from the raw Value, then uses WorldBuilder to produce
/// a fully materialized GameSnapshot with genre/world slugs set.
pub fn materialize_from_genre_pack(
    history_value: &serde_json::Value,
    maturity: CampaignMaturity,
    genre_slug: &str,
    world_slug: &str,
) -> Result<GameSnapshot, String> {
    let chapters = parse_history_chapters(history_value)?;

    let mut snap = WorldBuilder::new()
        .at_maturity(maturity)
        .with_chapters(chapters)
        .build();

    snap.genre_slug = genre_slug.to_string();
    snap.world_slug = world_slug.to_string();

    Ok(snap)
}
