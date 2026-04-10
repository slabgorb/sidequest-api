//! Character-related types: archetypes, creation scenes, visual style.

use super::ocean::OceanProfile;
use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// archetypes.yaml
// ═══════════════════════════════════════════════════════════

/// An NPC archetype template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NpcArchetype {
    /// Archetype name.
    pub name: NonBlankString,
    /// Description.
    pub description: String,
    /// Personality trait keywords.
    pub personality_traits: Vec<String>,
    /// Typical character classes.
    pub typical_classes: Vec<String>,
    /// Typical character races.
    pub typical_races: Vec<String>,
    /// Stat name → [min, max] ranges.
    pub stat_ranges: HashMap<String, [i32; 2]>,
    /// Starting inventory suggestions.
    pub inventory_hints: Vec<String>,
    /// Speech pattern descriptions.
    pub dialogue_quirks: Vec<String>,
    /// Default disposition value.
    pub disposition_default: i32,
    /// Item catalog references for starting gear.
    #[serde(default)]
    pub catalog_items: Vec<String>,
    /// Optional OCEAN personality baseline for this archetype.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocean: Option<OceanProfile>,
}

// ═══════════════════════════════════════════════════════════
// char_creation.yaml
// ═══════════════════════════════════════════════════════════

/// A character creation scene with narrative choices.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharCreationScene {
    /// Scene identifier.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Narrator text for this scene.
    pub narration: String,
    /// Player choices (may be empty for the final confirmation scene).
    pub choices: Vec<CharCreationChoice>,
    /// Genre-aware loading text shown while waiting for the next scene.
    /// E.g. "The ripperdoc considers your words..."
    #[serde(default)]
    pub loading_text: Option<String>,
    /// Whether this scene allows freeform text input.
    #[serde(default)]
    pub allows_freeform: Option<bool>,
    /// Optional followup prompt — if present, the builder enters AwaitingFollowup
    /// after a choice is made, waiting for the player's freeform elaboration.
    #[serde(default)]
    pub hook_prompt: Option<String>,
    /// Scene-level mechanical effects (e.g., stat_generation, equipment_generation).
    /// These are directives to the engine, not player-choice effects.
    #[serde(default)]
    pub mechanical_effects: Option<MechanicalEffects>,
}

/// A choice within a character creation scene.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharCreationChoice {
    /// Display label.
    pub label: String,
    /// Description text.
    pub description: String,
    /// Game-mechanical effects of this choice.
    pub mechanical_effects: MechanicalEffects,
}

/// Mechanical effects of a character creation choice or scene-level directive.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MechanicalEffects {
    /// Suggested class.
    #[serde(default)]
    pub class_hint: Option<String>,
    /// Suggested race.
    #[serde(default)]
    pub race_hint: Option<String>,
    /// Suggested mutation.
    #[serde(default)]
    pub mutation_hint: Option<String>,
    /// Suggested starting item.
    #[serde(default)]
    pub item_hint: Option<String>,
    /// Suggested affinity.
    #[serde(default)]
    pub affinity_hint: Option<String>,
    /// Suggested training path.
    #[serde(default)]
    pub training_hint: Option<String>,
    /// Background context.
    #[serde(default)]
    pub background: Option<String>,
    /// Personality trait hint.
    #[serde(default)]
    pub personality_trait: Option<String>,
    /// Emotional state hint.
    #[serde(default)]
    pub emotional_state: Option<String>,
    /// Relationship hint.
    #[serde(default)]
    pub relationship: Option<String>,
    /// Goals hint.
    #[serde(default)]
    pub goals: Option<String>,
    /// Whether freeform input is allowed.
    #[serde(default)]
    pub allows_freeform: Option<bool>,
    /// Rig type hint (vehicle-based genres).
    #[serde(default)]
    pub rig_type_hint: Option<String>,
    /// Rig trait hint.
    #[serde(default)]
    pub rig_trait: Option<String>,
    /// Catch phrase or hook.
    #[serde(default, rename = "catch")]
    pub catch_phrase: Option<String>,
    /// Stat bonuses from this choice (e.g. {"Strength": 2, "Agility": -1}).
    #[serde(default)]
    pub stat_bonuses: HashMap<String, i32>,
    /// Pronoun hint from character creation (e.g. "she/her", "he/him", "they/them").
    #[serde(default)]
    pub pronoun_hint: Option<String>,
    /// Stat generation method override (scene-level directive, e.g. "roll_3d6_strict").
    #[serde(default)]
    pub stat_generation: Option<String>,
    /// Equipment generation method (scene-level directive, e.g. "random_table").
    #[serde(default)]
    pub equipment_generation: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// backstory_tables.yaml
// ═══════════════════════════════════════════════════════════

/// Random backstory composition tables loaded from `backstory_tables.yaml`.
/// Each genre pack can optionally provide these for genres where character
/// creation doesn't produce backstory fragments (e.g., Caverns & Claudes).
///
/// YAML structure: `template` is a string, all other top-level keys are
/// tables (Vec<String>). We deserialize from a raw Value to handle the
/// mixed-type sibling keys.
#[derive(Debug, Clone, Serialize)]
pub struct BackstoryTables {
    /// Template string with `{key}` placeholders (e.g., "Former {trade}. {feature}. {reason}.").
    pub template: String,
    /// Named tables of random entries. Keys match placeholders in the template.
    pub tables: HashMap<String, Vec<String>>,
}

impl<'de> serde::Deserialize<'de> for BackstoryTables {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let raw: HashMap<String, serde_yaml::Value> = HashMap::deserialize(deserializer)?;

        let template = raw
            .get("template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("template"))?
            .to_string();

        let mut tables = HashMap::new();
        for (key, value) in &raw {
            if key == "template" {
                continue;
            }
            // Skip comment-only keys
            if let serde_yaml::Value::Sequence(seq) = value {
                let entries: Vec<String> = seq
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if !entries.is_empty() {
                    tables.insert(key.clone(), entries);
                }
            }
        }

        Ok(BackstoryTables { template, tables })
    }
}

// ═══════════════════════════════════════════════════════════
// equipment_tables.yaml
// ═══════════════════════════════════════════════════════════

/// Random equipment generation tables loaded from `equipment_tables.yaml`.
///
/// Consumed by `CharacterBuilder` when a character creation scene declares
/// `equipment_generation: random_table` in its `mechanical_effects`. Each
/// slot holds candidate item_ids; the builder rolls one (or `rolls_per_slot`)
/// item per slot and appends them to the starting inventory.
///
/// All referenced item_ids must resolve against the genre pack's
/// `inventory.item_catalog` — enforced by the wiring test, not by the
/// deserializer.
///
/// Story 31-3: Equipment generation wiring (final piece of Epic 31).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentTables {
    /// Slot name → candidate item_ids. Slot names are genre-defined
    /// (e.g., "weapon", "armor", "utility", "consumable") — not an enum.
    pub tables: HashMap<String, Vec<String>>,
    /// Optional per-slot roll count override. Slots not listed default to
    /// one roll. Example: `{ "light": 3, "consumable": 2 }` yields three
    /// torches and two rations.
    #[serde(default)]
    pub rolls_per_slot: HashMap<String, u32>,
}

// ═══════════════════════════════════════════════════════════
// visual_style.yaml
// ═══════════════════════════════════════════════════════════

/// Image generation style configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VisualStyle {
    /// Positive prompt suffix for image generation.
    pub positive_suffix: String,
    /// Negative prompt for image generation.
    pub negative_prompt: String,
    /// Preferred image generation model.
    pub preferred_model: String,
    /// Base random seed.
    pub base_seed: u32,
    /// Location-tag → style override mappings.
    #[serde(default)]
    pub visual_tag_overrides: HashMap<String, String>,
}
