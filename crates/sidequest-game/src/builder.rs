//! CharacterBuilder — state machine for genre-driven character creation.
//!
//! Story 2-3: Ports the Python CharacterBuilder as a typed state machine.
//! The builder doesn't exist before `new()` and is consumed conceptually by `build()`.
//! No IDLE or COMPLETE states — construction and consumption are the boundaries.

use std::collections::HashMap;

use rand::Rng;
use sidequest_genre::{
    BackstoryTables, CharCreationScene, EquipmentTables, MechanicalEffects, RulesConfig,
};
use tracing::info_span;
use sidequest_protocol::{CharacterCreationPayload, CreationChoice, GameMessage, NonBlankString, RolledStat};

use crate::character::Character;
use crate::creature_core::CreatureCore;
use crate::inventory::{Inventory, Item, ItemState};

// ============================================================================
// Public types
// ============================================================================

/// State machine phase for character creation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BuilderPhase {
    /// Processing genre-defined scenes.
    InProgress {
        /// Current scene index.
        scene_index: usize,
    },
    /// Scene has a hook_prompt — waiting for player's followup text.
    AwaitingFollowup {
        /// Scene index of the scene that triggered followup.
        scene_index: usize,
        /// The prompt to show the player.
        hook_prompt: String,
    },
    /// All scenes done, showing summary for confirmation.
    Confirmation,
}

/// How the player responded to a scene.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SceneInputType {
    /// Player selected a numbered choice.
    Choice(usize),
    /// Player typed freeform text.
    Freeform(String),
}

/// What a single scene produced — the unit of revert.
#[derive(Debug, Clone)]
pub struct SceneResult {
    /// How the player responded.
    pub input_type: SceneInputType,
    /// Narrative hooks extracted from this scene.
    pub hooks_added: Vec<NarrativeHook>,
    /// Lore anchors extracted from this scene.
    pub anchors_added: Vec<LoreAnchor>,
    /// Mechanical effects applied by this scene's choice.
    pub effects_applied: MechanicalEffects,
    /// The flavor description text from the chosen option (e.g. "A city built
    /// from stacked ruins…"). Stored here so we can compose a narrative backstory
    /// instead of only keeping the mechanical label.
    pub choice_description: Option<String>,
}

/// A narrative hook derived from character creation choices.
#[derive(Debug, Clone)]
pub struct NarrativeHook {
    /// Category of hook.
    pub hook_type: HookType,
    /// Which scene generated this hook.
    pub source_scene: String,
    /// Player-authored or choice-derived text.
    pub text: String,
    /// Effect key that generated it, if any.
    pub mechanical_key: Option<String>,
}

/// Category of narrative hook.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HookType {
    /// From race_hint.
    Origin,
    /// From backstory trauma.
    Wound,
    /// From relationship effects.
    Relationship,
    /// From goals effects.
    Goal,
    /// From class_hint or personality_trait.
    Trait,
    /// From obligation effects.
    Debt,
    /// From hidden knowledge.
    Secret,
    /// From equipment_hints / item_hint.
    Possession,
}

/// A connection to the game world (faction, NPC, location).
#[derive(Debug, Clone)]
pub struct LoreAnchor {
    /// Type of anchor: "faction", "npc_relationship", "location".
    pub anchor_type: String,
    /// The value — faction name, NPC name, or location name.
    pub value: String,
    /// Which scene generated this anchor.
    pub source_scene: String,
}

/// Accumulated mechanical effects across all completed scenes.
#[derive(Debug, Clone, Default)]
pub struct AccumulatedChoices {
    /// Accumulated class hint (last one wins).
    pub class_hint: Option<String>,
    /// Accumulated race hint (last one wins).
    pub race_hint: Option<String>,
    /// Accumulated personality trait (last one wins).
    pub personality_trait: Option<String>,
    /// Accumulated item hints.
    pub item_hints: Vec<String>,
    /// Accumulated affinity hint (last one wins).
    pub affinity_hint: Option<String>,
    /// Accumulated background (last one wins).
    pub background: Option<String>,
    /// Accumulated mutation hint (last one wins).
    pub mutation_hint: Option<String>,
    /// Accumulated training hint (last one wins).
    pub training_hint: Option<String>,
    /// Accumulated emotional state (last one wins).
    pub emotional_state: Option<String>,
    /// Accumulated relationship (last one wins).
    pub relationship: Option<String>,
    /// Accumulated goals (last one wins).
    pub goals: Option<String>,
    /// Accumulated rig type hint (vehicle genres, last one wins).
    pub rig_type_hint: Option<String>,
    /// Accumulated rig trait (vehicle genres, last one wins).
    pub rig_trait: Option<String>,
    /// Accumulated catch phrase (last one wins).
    pub catch_phrase: Option<String>,
    /// Rich description text from each creation choice, in scene order.
    /// Used to compose a narrative backstory instead of bare mechanical labels.
    pub backstory_fragments: Vec<String>,
    /// Accumulated stat bonuses from origin/mutation/artifact choices.
    pub stat_bonuses: HashMap<String, i32>,
    /// Accumulated pronoun hint (last one wins).
    pub pronoun_hint: Option<String>,
}

/// Errors from CharacterBuilder operations.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum BuilderError {
    /// Choice index out of range.
    #[error("invalid choice: index {index} but max is {max}")]
    InvalidChoice {
        /// The index the player chose.
        index: usize,
        /// The maximum valid index.
        max: usize,
    },
    /// Operation not valid in the current phase.
    #[error("wrong phase: expected {expected}, got {actual}")]
    WrongPhase {
        /// The phase required for this operation.
        expected: String,
        /// The phase the builder is actually in.
        actual: String,
    },
    /// Freeform input not allowed for this scene.
    #[error("freeform input not allowed for this scene")]
    FreeformNotAllowed,
    /// No scenes provided to the builder.
    #[error("no scenes provided")]
    NoScenes,
    /// Cannot revert — already at the first scene.
    #[error("cannot revert: already at first scene")]
    CannotRevert,
    /// Unrecognized stat generation method.
    #[error("unknown stat generation method: {0}")]
    UnknownStatGeneration(String),
    /// HP formula evaluation failed.
    #[error("hp_formula error: {0}")]
    InvalidHpFormula(String),
    /// Name is purely numeric — likely a UI index, not a real character name.
    #[error("invalid character name: '{0}' is purely numeric (likely a UI index, not a name)")]
    NumericName(String),
}

// ============================================================================
// CharacterBuilder
// ============================================================================

/// State machine for character creation driven by genre-pack scenes.
///
/// Tracks scene progression, accumulates mechanical effects, extracts
/// narrative hooks, and ultimately produces a `Character`.
pub struct CharacterBuilder {
    scenes: Vec<CharCreationScene>,
    results: Vec<SceneResult>,
    phase: BuilderPhase,
    // Stored config from RulesConfig
    stat_generation: String,
    ability_score_names: Vec<String>,
    default_class: Option<String>,
    default_race: Option<String>,
    default_hp: Option<u32>,
    default_ac: Option<u32>,
    class_hp_bases: HashMap<String, u32>,
    race_label: String,
    class_label: String,
    /// HP formula string from genre pack (e.g., "8 + CON_modifier").
    /// When present, overrides class_hp_bases lookup during build().
    hp_formula: Option<String>,
    /// Point budget for point-buy stat generation (D&D 5e standard: 27).
    point_buy_budget: u32,
    /// Pre-rolled stats for roll_3d6_strict (rolled eagerly at construction).
    /// Stored in ability_score_names order for narration injection.
    rolled_stats: Option<Vec<(String, i32)>>,
    /// Optional backstory random tables from the genre pack.
    backstory_tables: Option<BackstoryTables>,
    /// Optional equipment random tables from the genre pack. Story 31-3.
    /// When set AND a scene declares `equipment_generation: random_table`,
    /// the builder rolls one item per slot (or `rolls_per_slot` override).
    equipment_tables: Option<EquipmentTables>,
}

impl CharacterBuilder {
    /// Create a new builder. Panics if `scenes` is empty.
    pub fn new(
        scenes: Vec<CharCreationScene>,
        rules: &RulesConfig,
        backstory_tables: Option<BackstoryTables>,
    ) -> Self {
        assert!(
            !scenes.is_empty(),
            "CharacterBuilder requires at least one scene"
        );
        Self::build_inner(scenes, rules, backstory_tables)
    }

    /// Create a new builder, returning an error if `scenes` is empty.
    pub fn try_new(
        scenes: Vec<CharCreationScene>,
        rules: &RulesConfig,
        backstory_tables: Option<BackstoryTables>,
    ) -> Result<Self, BuilderError> {
        if scenes.is_empty() {
            return Err(BuilderError::NoScenes);
        }
        Ok(Self::build_inner(scenes, rules, backstory_tables))
    }

    fn build_inner(
        scenes: Vec<CharCreationScene>,
        rules: &RulesConfig,
        backstory_tables: Option<BackstoryTables>,
    ) -> Self {
        // Scan scenes for stat_generation directives — roll eagerly so stats
        // are available for narration injection when the scene is first shown.
        // The scene content is authoritative: if a scene declares
        // stat_generation: roll_3d6_strict, that scene's narration gets stat values.
        let rolled_stats = scenes
            .iter()
            .find_map(|s| {
                s.mechanical_effects
                    .as_ref()
                    .and_then(|e| e.stat_generation.as_deref())
            })
            .and_then(|method| match method {
                "roll_3d6_strict" => {
                    let mut rng = rand::rng();
                    Some(Self::roll_3d6_stats(&rules.ability_score_names, &mut rng))
                }
                _ => None,
            });

        Self {
            scenes,
            results: Vec::new(),
            phase: BuilderPhase::InProgress { scene_index: 0 },
            stat_generation: rules.stat_generation.clone(),
            ability_score_names: rules.ability_score_names.clone(),
            default_class: rules.default_class.clone(),
            default_race: rules.default_race.clone(),
            default_hp: rules.default_hp,
            default_ac: rules.default_ac,
            class_hp_bases: rules.class_hp_bases.clone(),
            hp_formula: rules.hp_formula.clone(),
            point_buy_budget: rules.point_buy_budget,
            race_label: rules
                .race_label
                .clone()
                .unwrap_or_else(|| "Race".to_string()),
            class_label: rules
                .class_label
                .clone()
                .unwrap_or_else(|| "Class".to_string()),
            rolled_stats,
            backstory_tables,
            equipment_tables: None,
        }
    }

    /// Attach an `EquipmentTables` to this builder. Fluent setter — chain
    /// after `new()` or `try_new()`. When set, scenes with
    /// `mechanical_effects.equipment_generation == Some("random_table")`
    /// will roll starting inventory from these tables during `build()`.
    ///
    /// Story 31-3: chose a setter over a 4th positional parameter to keep
    /// the blast radius small across the 52 existing `CharacterBuilder::new`
    /// call sites.
    pub fn with_equipment_tables(mut self, tables: EquipmentTables) -> Self {
        self.equipment_tables = Some(tables);
        self
    }

    /// Roll 3d6 for each ability score in order. Returns (name, total) pairs.
    fn roll_3d6_stats(
        ability_score_names: &[String],
        rng: &mut impl Rng,
    ) -> Vec<(String, i32)> {
        let results: Vec<(String, i32)> = ability_score_names
            .iter()
            .map(|name| {
                let dice: [i32; 3] = [
                    rng.random_range(1..=6),
                    rng.random_range(1..=6),
                    rng.random_range(1..=6),
                ];
                let total = dice.iter().sum();

                let span = info_span!(
                    "chargen.stat_roll",
                    method = "roll_3d6_strict",
                    stat_name = %name,
                    d1 = dice[0],
                    d2 = dice[1],
                    d3 = dice[2],
                    total = total,
                );
                let _guard = span.enter();

                (name.clone(), total)
            })
            .collect();

        let span = info_span!(
            "chargen.stats_generated",
            method = "roll_3d6_strict",
        );
        let _guard = span.enter();
        for (name, val) in &results {
            tracing::info!(stat = %name, value = val, "stat generated");
        }

        results
    }

    /// Allocate a point-buy budget across `n` stats using the D&D 5e cost table.
    ///
    /// All stats start at 8. Points are distributed round-robin, raising each
    /// stat by 1 point at a time (cheapest-first) until the budget is spent.
    /// This produces a balanced spread — no dump stats, no extreme min-maxing.
    ///
    /// Cost table (cumulative from base 8):
    ///   8→9: 1, 9→10: 1, 10→11: 1, 11→12: 1, 12→13: 1, 13→14: 2, 14→15: 2
    fn allocate_point_buy(n: usize, budget: u32) -> Vec<i32> {
        // Cost to go from (value - 1) to value
        fn marginal_cost(value: i32) -> u32 {
            match value {
                9..=13 => 1,
                14 | 15 => 2,
                _ => u32::MAX, // Can't go below 8 or above 15
            }
        }

        let mut stats = vec![8i32; n];
        let mut remaining = budget;

        // Round-robin: raise each stat by 1, cycling until budget exhausted
        'outer: loop {
            let mut any_raised = false;
            for stat in stats.iter_mut() {
                let next_val = *stat + 1;
                if next_val > 15 {
                    continue;
                }
                let cost = marginal_cost(next_val);
                if cost <= remaining {
                    *stat = next_val;
                    remaining -= cost;
                    any_raised = true;
                }
            }
            if !any_raised || remaining == 0 {
                break 'outer;
            }
        }

        stats
    }

    // --- Phase queries ---

    /// Whether the builder is in InProgress phase.
    pub fn is_in_progress(&self) -> bool {
        matches!(self.phase, BuilderPhase::InProgress { .. })
    }

    /// Whether the builder is awaiting a followup answer.
    pub fn is_awaiting_followup(&self) -> bool {
        matches!(self.phase, BuilderPhase::AwaitingFollowup { .. })
    }

    /// Whether the builder is in Confirmation phase.
    pub fn is_confirmation(&self) -> bool {
        matches!(self.phase, BuilderPhase::Confirmation)
    }

    /// Current scene index (0-based).
    pub fn current_scene_index(&self) -> usize {
        match &self.phase {
            BuilderPhase::InProgress { scene_index } => *scene_index,
            BuilderPhase::AwaitingFollowup { scene_index, .. } => *scene_index,
            BuilderPhase::Confirmation => self.scenes.len(),
        }
    }

    /// Reference to the current scene definition.
    pub fn current_scene(&self) -> &CharCreationScene {
        let idx = self.current_scene_index();
        &self.scenes[idx]
    }

    /// Total number of scenes.
    pub fn total_scenes(&self) -> usize {
        self.scenes.len()
    }

    /// Access the raw scene definitions (used for lore seeding).
    pub fn scenes(&self) -> &[CharCreationScene] {
        &self.scenes
    }

    /// The accumulated scene results stack.
    pub fn scene_results(&self) -> &[SceneResult] {
        &self.results
    }

    /// Extract the character name from the name-entry scene (last scene with
    /// no choices where the player typed freeform text).
    pub fn character_name(&self) -> Option<&str> {
        // The name scene is the last scene with no choices
        if let Some(last_scene) = self.scenes.last() {
            if last_scene.choices.is_empty() {
                // Find the corresponding result (last result)
                if let Some(result) = self.results.last() {
                    if let SceneInputType::Freeform(ref text) = result.input_type {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed);
                        }
                    }
                }
            }
        }
        None
    }

    /// Auto-advance through the current scene without client input.
    /// For display-only scenes (no choices, no freeform): applies scene-level
    /// mechanical effects and advances to the next scene.
    /// Returns Ok(()) if advanced, Err if the scene requires input.
    pub fn apply_auto_advance(&mut self) -> Result<(), BuilderError> {
        let scene_index = match &self.phase {
            BuilderPhase::InProgress { scene_index } => *scene_index,
            _ => {
                return Err(BuilderError::WrongPhase {
                    expected: "InProgress".to_string(),
                    actual: self.phase_name().to_string(),
                });
            }
        };

        let scene = &self.scenes[scene_index];
        if !scene.choices.is_empty() || scene.allows_freeform.unwrap_or(false) {
            return Err(BuilderError::InvalidChoice {
                index: 0,
                max: scene.choices.len(),
            });
        }

        // Apply scene-level mechanical effects (stat rolling, equipment gen, etc.)
        let effects = scene
            .mechanical_effects
            .clone()
            .unwrap_or_default();

        // Record as an auto-advanced scene result
        self.results.push(SceneResult {
            input_type: SceneInputType::Choice(0),
            hooks_added: vec![],
            anchors_added: vec![],
            effects_applied: effects.clone(),
            choice_description: None,
        });

        // Apply stat generation if specified
        if let Some(ref method) = effects.stat_generation {
            match method.as_str() {
                "roll_3d6_strict" => {
                    if self.rolled_stats.is_none() {
                        let mut rng = rand::rng();
                        self.rolled_stats =
                            Some(Self::roll_3d6_stats(&self.ability_score_names, &mut rng));
                    }
                }
                other => {
                    self.stat_generation = other.to_string();
                }
            }
        }

        tracing::info!(
            scene_id = %scene.id,
            scene_index = scene_index,
            "chargen.auto_advance — choiceless scene"
        );

        self.advance_scene(scene_index);
        Ok(())
    }

    /// Get the current hook prompt text, if awaiting followup.
    pub fn current_hook_prompt(&self) -> Option<&str> {
        match &self.phase {
            BuilderPhase::AwaitingFollowup { hook_prompt, .. } => Some(hook_prompt.as_str()),
            _ => None,
        }
    }

    /// Compute accumulated choices from scene results.
    pub fn accumulated(&self) -> AccumulatedChoices {
        let mut acc = AccumulatedChoices::default();
        for result in &self.results {
            let eff = &result.effects_applied;
            if let Some(ref v) = eff.class_hint {
                acc.class_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.race_hint {
                acc.race_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.personality_trait {
                acc.personality_trait = Some(v.clone());
            }
            if let Some(ref v) = eff.affinity_hint {
                acc.affinity_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.background {
                acc.background = Some(v.clone());
            }
            if let Some(ref v) = eff.item_hint {
                if !v.is_empty() && v != "none" {
                    acc.item_hints.push(v.clone());
                }
            }
            if let Some(ref v) = eff.mutation_hint {
                acc.mutation_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.training_hint {
                acc.training_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.emotional_state {
                acc.emotional_state = Some(v.clone());
            }
            if let Some(ref v) = eff.relationship {
                acc.relationship = Some(v.clone());
            }
            if let Some(ref v) = eff.goals {
                acc.goals = Some(v.clone());
            }
            if let Some(ref v) = eff.rig_type_hint {
                acc.rig_type_hint = Some(v.clone());
            }
            if let Some(ref v) = eff.rig_trait {
                acc.rig_trait = Some(v.clone());
            }
            if let Some(ref v) = eff.catch_phrase {
                acc.catch_phrase = Some(v.clone());
            }
            if let Some(ref v) = eff.pronoun_hint {
                acc.pronoun_hint = Some(v.clone());
            }
            // Collect the rich description text from each choice for backstory.
            // Skip pronoun-only choices — their description (e.g., "He.") is not
            // a backstory fragment.
            if let Some(ref desc) = result.choice_description {
                let is_pronoun_only = eff.pronoun_hint.is_some()
                    && eff.class_hint.is_none()
                    && eff.race_hint.is_none()
                    && eff.background.is_none()
                    && eff.personality_trait.is_none()
                    && eff.relationship.is_none()
                    && eff.goals.is_none();
                if !is_pronoun_only {
                    acc.backstory_fragments.push(desc.clone());
                }
            }
            // Accumulate stat bonuses (additive across all scenes)
            for (stat, bonus) in &eff.stat_bonuses {
                *acc.stat_bonuses.entry(stat.clone()).or_insert(0) += bonus;
            }
        }
        acc
    }

    // --- Actions ---

    /// Apply a numbered choice to the current scene.
    pub fn apply_choice(&mut self, index: usize) -> Result<(), BuilderError> {
        let scene_index = match &self.phase {
            BuilderPhase::InProgress { scene_index } => *scene_index,
            BuilderPhase::AwaitingFollowup { .. } => {
                return Err(BuilderError::WrongPhase {
                    expected: "InProgress".to_string(),
                    actual: "AwaitingFollowup".to_string(),
                });
            }
            BuilderPhase::Confirmation => {
                return Err(BuilderError::WrongPhase {
                    expected: "InProgress".to_string(),
                    actual: "Confirmation".to_string(),
                });
            }
        };

        let scene = &self.scenes[scene_index];
        if index >= scene.choices.len() {
            return Err(BuilderError::InvalidChoice {
                index,
                max: scene.choices.len().saturating_sub(1),
            });
        }

        let choice = &scene.choices[index];
        let effects = choice.mechanical_effects.clone();
        let hooks = extract_hooks(&scene.id, &effects);
        let anchors = extract_anchors(&scene.id, &effects);
        let description = Some(choice.description.clone());

        self.results.push(SceneResult {
            input_type: SceneInputType::Choice(index),
            hooks_added: hooks,
            anchors_added: anchors,
            effects_applied: effects,
            choice_description: description,
        });

        // Check for hook_prompt → AwaitingFollowup, else advance
        if let Some(ref prompt) = scene.hook_prompt {
            self.phase = BuilderPhase::AwaitingFollowup {
                scene_index,
                hook_prompt: prompt.clone(),
            };
        } else {
            self.advance_scene(scene_index);
        }

        Ok(())
    }

    /// Apply freeform text input to the current scene.
    pub fn apply_freeform(&mut self, text: &str) -> Result<(), BuilderError> {
        let scene_index = match &self.phase {
            BuilderPhase::InProgress { scene_index } => *scene_index,
            _ => {
                return Err(BuilderError::WrongPhase {
                    expected: "InProgress".to_string(),
                    actual: self.phase_name().to_string(),
                });
            }
        };

        let scene = &self.scenes[scene_index];
        // Allow freeform for scenes that explicitly allow it OR for scenes with
        // no choices (name-entry scenes at the end of chargen).
        if scene.allows_freeform != Some(true) && !scene.choices.is_empty() {
            return Err(BuilderError::FreeformNotAllowed);
        }

        // Use scene-level mechanical_effects if present (e.g., the_roll has
        // stat_generation, the_kit has equipment_generation). Otherwise empty.
        let effects = scene
            .mechanical_effects
            .clone()
            .unwrap_or_default();

        // Process scene-level stat_generation directive
        if let Some(ref method) = effects.stat_generation {
            match method.as_str() {
                "roll_3d6_strict" => {
                    let mut rng = rand::rng();
                    self.rolled_stats =
                        Some(Self::roll_3d6_stats(&self.ability_score_names, &mut rng));
                }
                other => {
                    // Override the builder's stat_generation from scene directive
                    self.stat_generation = other.to_string();
                }
            }
        }

        let hooks = extract_hooks(&scene.id, &effects);
        let anchors = extract_anchors(&scene.id, &effects);

        self.results.push(SceneResult {
            input_type: SceneInputType::Freeform(text.to_string()),
            hooks_added: hooks,
            anchors_added: anchors,
            effects_applied: effects,
            choice_description: None,
        });

        if let Some(ref prompt) = scene.hook_prompt {
            self.phase = BuilderPhase::AwaitingFollowup {
                scene_index,
                hook_prompt: prompt.clone(),
            };
        } else {
            self.advance_scene(scene_index);
        }

        Ok(())
    }

    /// Answer a followup prompt while in AwaitingFollowup state.
    pub fn answer_followup(&mut self, text: &str) -> Result<(), BuilderError> {
        let scene_index = match &self.phase {
            BuilderPhase::AwaitingFollowup { scene_index, .. } => *scene_index,
            _ => {
                return Err(BuilderError::WrongPhase {
                    expected: "AwaitingFollowup".to_string(),
                    actual: self.phase_name().to_string(),
                });
            }
        };

        // Insert the followup hook at position 0 — it's the player's primary hook
        let scene_id = self.scenes[scene_index].id.clone();
        if let Some(last) = self.results.last_mut() {
            last.hooks_added.insert(
                0,
                NarrativeHook {
                    hook_type: HookType::Wound,
                    source_scene: scene_id,
                    text: text.to_string(),
                    mechanical_key: None,
                },
            );
        }

        self.advance_scene(scene_index);
        Ok(())
    }

    /// Revert the last scene — pop the SceneResult and go back.
    pub fn revert(&mut self) -> Result<(), BuilderError> {
        if self.results.is_empty() {
            return Err(BuilderError::CannotRevert);
        }

        self.results.pop();
        let new_index = self.results.len();
        self.phase = BuilderPhase::InProgress {
            scene_index: new_index,
        };

        Ok(())
    }

    /// Build the final Character from accumulated choices.
    ///
    /// Only valid from Confirmation phase.
    pub fn build(&mut self, name: &str) -> Result<Character, BuilderError> {
        if !self.is_confirmation() {
            return Err(BuilderError::WrongPhase {
                expected: "Confirmation".to_string(),
                actual: self.phase_name().to_string(),
            });
        }

        // Story 30-1: Reject purely numeric names — they indicate a UI choice
        // index was used as the name fallback instead of a real character name.
        let trimmed = name.trim();
        if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
            return Err(BuilderError::NumericName(trimmed.to_string()));
        }

        let acc = self.accumulated();

        let race_str = acc
            .race_hint
            .as_deref()
            .or(self.default_race.as_deref())
            .unwrap_or("Human");
        let class_str = acc
            .class_hint
            .as_deref()
            .or(self.default_class.as_deref())
            .unwrap_or("Fighter");

        // Stats
        let stats = self.generate_stats(&acc)?;

        // HP from hp_formula or class_hp_bases fallback
        let base_hp = if let Some(ref formula) = self.hp_formula {
            let hp_result = Self::evaluate_hp_formula(formula, &stats, &self.class_hp_bases, class_str)?;
            let con_mod = stats.get("CON").map(|&v| (v - 10) / 2);
            let _span = info_span!(
                "chargen.hp_formula",
                formula = formula.as_str(),
                class = class_str,
                hp_result = hp_result,
                con_modifier = ?con_mod,
            )
            .entered();
            hp_result
        } else {
            self.class_hp_bases
                .get(class_str)
                .copied()
                .or(self.default_hp)
                .unwrap_or(10) as i32
        };

        let ac = self.default_ac.unwrap_or(10) as i32;

        // Hooks: collect narrative hooks, excluding mechanical traits already on the sheet
        let excluded_keys = ["race_hint", "class_hint", "personality_trait"];
        let mut hooks: Vec<String> = Vec::new();
        for result in &self.results {
            for hook in &result.hooks_added {
                let dominated = hook
                    .mechanical_key
                    .as_deref()
                    .map_or(false, |k| excluded_keys.contains(&k));
                if !dominated {
                    hooks.push(hook.text.clone());
                }
            }
        }

        // Auto-fill lore anchors for faction, npc, location
        let anchor_types = ["faction", "npc", "location"];
        for anchor_type in &anchor_types {
            let has_anchor = self.results.iter().any(|r| {
                r.anchors_added
                    .iter()
                    .any(|a| a.anchor_type == *anchor_type)
            });
            if !has_anchor {
                hooks.push(format!("{}: auto-filled from genre pack", anchor_type));
            }
        }

        // Inventory composition: item hints first, then random equipment tables
        // when a scene directive opts in. Story 31-3.
        let mut items: Vec<Item> = acc
            .item_hints
            .iter()
            .enumerate()
            .map(|(i, hint)| {
                let id_str = hint.to_lowercase().replace(' ', "_");
                let display_name = humanize_snake_case(hint);
                Item {
                    id: NonBlankString::new(&id_str)
                        .unwrap_or_else(|_| NonBlankString::new(&format!("item_{}", i)).unwrap()),
                    name: NonBlankString::new(&display_name)
                        .unwrap_or_else(|_| NonBlankString::new("Unknown Item").unwrap()),
                    description: NonBlankString::new(&format!("Starting equipment: {}", display_name))
                        .unwrap(),
                    category: NonBlankString::new("weapon").unwrap(),
                    value: 10,
                    weight: 3.0,
                    rarity: NonBlankString::new("common").unwrap(),
                    narrative_weight: 0.3,
                    tags: vec![],
                    equipped: true,
                    quantity: 1,
                    uses_remaining: None,
                    state: ItemState::Carried,
                }
            })
            .collect();

        // Equipment random tables: apply when any scene declared
        // `equipment_generation: random_table` AND tables are wired in.
        // Story 31-3: wire genre-pack equipment_tables into CharacterBuilder.
        let random_table_requested = self.results.iter().any(|r| {
            r.effects_applied
                .equipment_generation
                .as_deref()
                == Some("random_table")
        });
        let (equipment_method, equipment_added) =
            if random_table_requested && self.equipment_tables.is_some() {
                let tables = self.equipment_tables.as_ref().unwrap();
                let mut rng = rand::rng();
                let mut added = 0usize;
                for (slot, candidates) in &tables.tables {
                    if candidates.is_empty() {
                        continue;
                    }
                    let rolls = tables.rolls_per_slot.get(slot).copied().unwrap_or(1);
                    for _ in 0..rolls {
                        let pick = &candidates[rng.random_range(0..candidates.len())];
                        let display_name = humanize_snake_case(pick);
                        let id = match NonBlankString::new(pick) {
                            Ok(id) => id,
                            Err(_) => {
                                // Blank id — skip, but record it so the GM panel
                                // can surface a malformed equipment_tables.yaml
                                // entry rather than silently producing short
                                // inventories.
                                tracing::warn!(
                                    slot = %slot,
                                    pick = %pick,
                                    "chargen.equipment_tables: blank item id, skipping"
                                );
                                continue;
                            }
                        };
                        items.push(Item {
                            id,
                            name: NonBlankString::new(&display_name)
                                .unwrap_or_else(|_| NonBlankString::new("Unknown Item").unwrap()),
                            description: NonBlankString::new(&format!(
                                "Starting equipment ({}): {}",
                                slot, display_name
                            ))
                            .unwrap(),
                            category: NonBlankString::new(slot).unwrap_or_else(|_| {
                                NonBlankString::new("misc").unwrap()
                            }),
                            value: 0,
                            weight: 1.0,
                            rarity: NonBlankString::new("common").unwrap(),
                            narrative_weight: 0.3,
                            tags: vec![],
                            equipped: false,
                            quantity: 1,
                            uses_remaining: None,
                            state: ItemState::Carried,
                        });
                        added += 1;
                    }
                }
                ("tables", added)
            } else if random_table_requested {
                // Directive present but no tables wired — silent no-op is
                // intentional here because the directive is optional content,
                // not a configuration error. OTEL event records the decision.
                ("none", 0)
            } else {
                ("hints", 0)
            };

        // OTEL: chargen.equipment_composed — GM panel verification that the
        // equipment subsystem engaged (or didn't, and why). Emitted as a
        // tracing event (not a span) since there is no meaningful duration
        // to measure — this is a point-in-time composition decision.
        tracing::info!(
            target: "chargen.equipment_composed",
            method = equipment_method,
            items_added = equipment_added,
        );

        // Compose backstory: fragments → tables → mechanical labels → fallback
        let backstory_text = if !acc.backstory_fragments.is_empty() {
            let _span = info_span!("chargen.backstory_composed", method = "fragments").entered();
            acc.backstory_fragments.join(" ")
        } else if let Some(ref tables) = self.backstory_tables {
            let _span = info_span!("chargen.backstory_composed", method = "tables").entered();
            let mut rng = rand::rng();
            let mut result = tables.template.clone();
            for (key, entries) in &tables.tables {
                if !entries.is_empty() {
                    let pick = &entries[rng.random_range(0..entries.len())];
                    result = result.replace(&format!("{{{}}}", key), pick);
                }
            }
            result
        } else {
            let _span = info_span!("chargen.backstory_composed", method = "fallback").entered();
            let mut parts = Vec::new();
            if let Some(ref bg) = acc.background {
                parts.push(format!("Background: {}", bg));
            }
            if let Some(ref pt) = acc.personality_trait {
                parts.push(format!("Personality: {}", pt));
            }
            if parts.is_empty() {
                "A wanderer with a mysterious past".to_string()
            } else {
                parts.join(". ")
            }
        };

        let character = Character {
            core: CreatureCore {
                name: NonBlankString::new(name).map_err(|_| BuilderError::WrongPhase {
                    expected: "valid name".to_string(),
                    actual: "blank name".to_string(),
                })?,
                description: NonBlankString::new(&format!("A {} {}", race_str, class_str)).unwrap(),
                personality: NonBlankString::new(
                    acc.personality_trait.as_deref().unwrap_or("Determined"),
                )
                .unwrap(),
                level: 1,
                hp: base_hp,
                max_hp: base_hp,
                ac,
                xp: 0,
                inventory: Inventory { items, gold: 0 },
                statuses: vec![],
            },
            backstory: NonBlankString::new(&backstory_text).unwrap(),
            narrative_state: "Beginning their adventure".to_string(),
            hooks,
            char_class: NonBlankString::new(class_str).unwrap(),
            race: NonBlankString::new(race_str).unwrap(),
            pronouns: acc.pronoun_hint.unwrap_or_default(),
            stats,
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
        };

        Ok(character)
    }

    /// Construct a CharacterCreation GameMessage for the current state.
    pub fn to_scene_message(&self, player_id: &str) -> GameMessage {
        match &self.phase {
            BuilderPhase::InProgress { scene_index } => {
                let scene = &self.scenes[*scene_index];
                let choices: Vec<CreationChoice> = scene
                    .choices
                    .iter()
                    .map(|c| CreationChoice {
                        label: c.label.clone(),
                        description: c.description.clone(),
                    })
                    .collect();

                // Disambiguate scenes with no choices:
                //   empty choices + allows_freeform=true  → name/text entry
                //   empty choices + allows_freeform=false → display-only "continue" scene
                //                                          (player acknowledges, no input)
                //   non-empty choices                     → choice (with optional freeform alt)
                let scene_allows_freeform = scene.allows_freeform.unwrap_or(false);
                let (input_type, allows_freeform) = if choices.is_empty() {
                    if scene_allows_freeform {
                        // Freeform input scene (typically the name-entry scene)
                        ("name".to_string(), Some(true))
                    } else {
                        // Display-only scene — narrate, wait for Continue ack
                        ("continue".to_string(), Some(false))
                    }
                } else {
                    ("choice".to_string(), scene.allows_freeform)
                };

                // Inject rolled stat values into narration for the scene that
                // declares stat_generation in its mechanical_effects.
                let scene_has_stat_gen = scene
                    .mechanical_effects
                    .as_ref()
                    .and_then(|e| e.stat_generation.as_ref())
                    .is_some();

                // Send rolled stats as STRUCTURED data (not inline markdown).
                // The UI renders them as a stat block alongside the narration.
                // The narration text stays clean — the player isn't asked to
                // parse "**STR 10** · **DEX 13** · ..." out of prose.
                let rolled_stats_payload = if scene_has_stat_gen {
                    self.rolled_stats.as_ref().map(|rolled| {
                        rolled
                            .iter()
                            .map(|(name, val)| RolledStat {
                                name: name.clone(),
                                value: *val,
                            })
                            .collect::<Vec<_>>()
                    })
                } else {
                    None
                };

                GameMessage::CharacterCreation {
                    payload: CharacterCreationPayload {
                        phase: "scene".to_string(),
                        scene_index: Some(*scene_index as u32),
                        total_scenes: Some(self.scenes.len() as u32),
                        prompt: Some(scene.narration.clone()),
                        summary: None,
                        message: None,
                        choices: Some(choices),
                        allows_freeform,
                        input_type: Some(input_type),
                        loading_text: scene.loading_text.clone(),
                        character_preview: None,
                        rolled_stats: rolled_stats_payload,
                        choice: None,
                        character: None,
                    },
                    player_id: player_id.to_string(),
                }
            }
            BuilderPhase::AwaitingFollowup { hook_prompt, .. } => GameMessage::CharacterCreation {
                payload: CharacterCreationPayload {
                    phase: "scene".to_string(),
                    scene_index: None,
                    total_scenes: Some(self.scenes.len() as u32),
                    prompt: Some(hook_prompt.clone()),
                    summary: None,
                    message: None,
                    choices: None,
                    allows_freeform: Some(true),
                    input_type: Some("text".to_string()),
                    loading_text: None,
                    character_preview: None,
                    rolled_stats: None,
                    choice: None,
                    character: None,
                },
                player_id: player_id.to_string(),
            },
            BuilderPhase::Confirmation => {
                let acc = self.accumulated();
                let mut parts = Vec::new();
                if let Some(name) = self.character_name() {
                    parts.push(format!("Name: {}", name));
                }
                // Only show race/class/personality if the chargen accumulated a value.
                // Genres like caverns_and_claudes deliberately omit these — don't lie
                // with "Unknown" for fields the genre doesn't define.
                if let Some(ref r) = acc.race_hint {
                    parts.push(format!("{}: {}", self.race_label, r));
                }
                if let Some(ref c) = acc.class_hint {
                    parts.push(format!("{}: {}", self.class_label, c));
                }
                if let Some(ref p) = acc.personality_trait {
                    parts.push(format!("Personality: {}", p));
                }
                if let Some(ref pn) = acc.pronoun_hint {
                    parts.push(format!("Pronouns: {}", pn));
                }
                if let Some(ref rolled) = self.rolled_stats {
                    let stat_line = rolled
                        .iter()
                        .map(|(name, val)| format!("{} {}", name, val))
                        .collect::<Vec<_>>()
                        .join("  ");
                    parts.push(format!("Stats: {}", stat_line));
                }
                if let Some(ref m) = acc.mutation_hint {
                    parts.push(format!("Mutation: {}", humanize_snake_case(m)));
                }
                if let Some(ref a) = acc.affinity_hint {
                    parts.push(format!("Affinity: {}", a));
                }
                if let Some(ref r) = acc.rig_type_hint {
                    parts.push(format!("Rig: {}", r));
                }
                if let Some(ref rt) = acc.rig_trait {
                    parts.push(format!("Rig Trait: {}", rt));
                }
                if !acc.item_hints.is_empty() {
                    let display_items: Vec<String> = acc.item_hints.iter()
                        .map(|h| humanize_snake_case(h))
                        .collect();
                    parts.push(format!("Equipment: {}", display_items.join(", ")));
                }
                if let Some(bg) = &acc.background {
                    parts.push(format!("\nBackstory: {}", bg));
                }
                let summary = parts.join("\n");

                GameMessage::CharacterCreation {
                    payload: CharacterCreationPayload {
                        phase: "confirmation".to_string(),
                        scene_index: None,
                        total_scenes: Some(self.scenes.len() as u32),
                        prompt: None,
                        summary: Some(summary),
                        message: None,
                        choices: None,
                        allows_freeform: None,
                        input_type: None,
                        loading_text: None,
                        character_preview: None,
                        rolled_stats: None,
                        choice: None,
                        character: None,
                    },
                    player_id: player_id.to_string(),
                }
            }
        }
    }

    // --- Private helpers ---

    fn advance_scene(&mut self, current: usize) {
        let next = current + 1;
        if next >= self.scenes.len() {
            self.phase = BuilderPhase::Confirmation;
        } else {
            self.phase = BuilderPhase::InProgress { scene_index: next };
        }
    }

    fn phase_name(&self) -> &str {
        match &self.phase {
            BuilderPhase::InProgress { .. } => "InProgress",
            BuilderPhase::AwaitingFollowup { .. } => "AwaitingFollowup",
            BuilderPhase::Confirmation => "Confirmation",
        }
    }

    fn generate_stats(
        &self,
        acc: &AccumulatedChoices,
    ) -> Result<HashMap<String, i32>, BuilderError> {
        let mut stats: HashMap<String, i32> = match self.stat_generation.as_str() {
            "roll_3d6_strict" => {
                // Use pre-rolled stats from construction
                if let Some(ref rolled) = self.rolled_stats {
                    rolled.iter().cloned().collect()
                } else {
                    // Fallback: roll now (shouldn't happen — rolled eagerly)
                    let mut rng = rand::rng();
                    Self::roll_3d6_stats(&self.ability_score_names, &mut rng)
                        .into_iter()
                        .collect()
                }
            }
            "standard_array" => {
                let base_values = vec![15, 14, 13, 12, 10, 8];
                self.ability_score_names
                    .iter()
                    .zip(base_values.into_iter())
                    .map(|(name, val)| (name.clone(), val))
                    .collect()
            }
            "point_buy" => {
                let values = Self::allocate_point_buy(
                    self.ability_score_names.len(),
                    self.point_buy_budget,
                );
                tracing::info!(
                    method = "point_buy",
                    budget = self.point_buy_budget,
                    stats = ?values,
                    "chargen.stats_generated"
                );
                self.ability_score_names
                    .iter()
                    .zip(values.into_iter())
                    .map(|(name, val)| (name.clone(), val))
                    .collect()
            }
            other => {
                return Err(BuilderError::UnknownStatGeneration(other.to_string()));
            }
        };

        // Apply explicit stat bonuses from genre pack choices (origin, mutation, artifact)
        for (stat, bonus) in &acc.stat_bonuses {
            if let Some(val) = stats.get_mut(stat) {
                *val += bonus;
            }
        }

        // If no explicit bonuses were set and we're using standard_array,
        // derive differentiation from the player's accumulated choices.
        if acc.stat_bonuses.is_empty()
            && self.stat_generation == "standard_array"
            && self.ability_score_names.len() >= 3
        {
            let names = &self.ability_score_names;
            // Origin/race → boost first stat
            if acc.race_hint.is_some() {
                if let Some(val) = stats.get_mut(&names[0]) {
                    *val += 3;
                }
            }
            // Mutation/affinity → boost second stat, reduce last
            if acc.mutation_hint.is_some() || acc.affinity_hint.is_some() {
                if let Some(val) = stats.get_mut(&names[1]) {
                    *val += 2;
                }
                if let Some(val) = stats.get_mut(&names[names.len() - 1]) {
                    *val -= 1;
                }
            }
            // Class/training → boost third stat
            if acc.class_hint.is_some() || acc.training_hint.is_some() {
                let idx = 2.min(names.len() - 1);
                if let Some(val) = stats.get_mut(&names[idx]) {
                    *val += 2;
                }
            }
        }

        Ok(stats)
    }

    /// Evaluate an hp_formula string using rolled stats and class config.
    ///
    /// Supported variables:
    /// - `XXX_modifier` — D&D-style ability modifier: floor((stat - 10) / 2)
    ///   where XXX matches any key in the stats HashMap (e.g., CON, STR, body)
    /// - `class_base` — class_hp_bases lookup for the current class
    /// - `level` — character level (always 1 at creation)
    /// - Integer literals
    ///
    /// Supported operators: `+`, `-`, `*` (left-to-right, no precedence beyond parens)
    /// Parentheses are stripped before evaluation.
    ///
    /// Returns `Err` on unrecognized tokens, missing variables, or empty formulas.
    fn evaluate_hp_formula(
        formula: &str,
        stats: &HashMap<String, i32>,
        class_hp_bases: &HashMap<String, u32>,
        class_str: &str,
    ) -> Result<i32, BuilderError> {
        if formula.trim().is_empty() {
            return Err(BuilderError::InvalidHpFormula(
                "hp_formula is empty".to_string(),
            ));
        }

        // Build variable substitution table
        let class_base = class_hp_bases.get(class_str).copied().unwrap_or(8) as i32;
        let level: i32 = 1; // Always 1 at character creation

        // Substitute variables in the formula string
        let mut expr = formula.to_string();

        // Replace XXX_modifier patterns (e.g., CON_modifier, body_mod, nerve_mod)
        // Check for full _modifier suffix first, then _mod suffix
        for (stat_name, &stat_value) in stats {
            let modifier = (stat_value - 10) / 2;
            let modifier_var = format!("{}_modifier", stat_name);
            let mod_var = format!("{}_mod", stat_name.to_lowercase());
            expr = expr.replace(&modifier_var, &modifier.to_string());
            expr = expr.replace(&mod_var, &modifier.to_string());
        }

        // Replace class_base and level
        expr = expr.replace("class_base", &class_base.to_string());
        expr = expr.replace("level", &level.to_string());

        // Strip parentheses (simple formulas only)
        expr = expr.replace('(', "").replace(')', "");

        // Evaluate the arithmetic expression (supports +, -, *)
        let result = Self::eval_simple_arithmetic(&expr).map_err(|token| {
            BuilderError::InvalidHpFormula(format!(
                "unparseable token '{}' in formula '{}' (after substitution: '{}')",
                token, formula, expr
            ))
        })?;

        // Floor at 1 — no zero or negative HP
        Ok(result.max(1))
    }

    /// Evaluate a simple arithmetic expression with +, -, * operators.
    /// No operator precedence — evaluates left to right.
    /// Handles negative numbers from variable substitution.
    ///
    /// Returns `Err(token)` if any token fails to parse as i32.
    fn eval_simple_arithmetic(expr: &str) -> Result<i32, String> {
        let expr = expr.trim();

        // Tokenize: split on operators while preserving them.
        // A '-' at the start of the expression (or after an operator) is part
        // of a negative literal, not a binary operator.
        let mut tokens: Vec<String> = Vec::new();
        let mut current = String::new();

        for ch in expr.chars() {
            if (ch == '+' || ch == '-' || ch == '*') && !current.trim().is_empty() {
                tokens.push(current.trim().to_string());
                tokens.push(ch.to_string());
                current = String::new();
            } else {
                current.push(ch);
            }
        }
        if !current.trim().is_empty() {
            tokens.push(current.trim().to_string());
        }

        if tokens.is_empty() {
            return Err("empty expression".to_string());
        }

        // Evaluate left to right
        let mut result: i32 = tokens[0]
            .parse()
            .map_err(|_| tokens[0].clone())?;
        let mut i = 1;
        while i + 1 < tokens.len() {
            let op = &tokens[i];
            let operand: i32 = tokens[i + 1]
                .parse()
                .map_err(|_| tokens[i + 1].clone())?;
            match op.as_str() {
                "+" => result += operand,
                "-" => result -= operand,
                "*" => result *= operand,
                other => return Err(format!("unknown operator '{}'", other)),
            }
            i += 2;
        }

        Ok(result)
    }
}

// ============================================================================
// Hook extraction
// ============================================================================

fn extract_hooks(scene_id: &str, effects: &MechanicalEffects) -> Vec<NarrativeHook> {
    let mut hooks = Vec::new();

    if let Some(ref v) = effects.race_hint {
        hooks.push(NarrativeHook {
            hook_type: HookType::Origin,
            source_scene: scene_id.to_string(),
            text: format!("Origin: {}", v),
            mechanical_key: Some("race_hint".to_string()),
        });
    }

    if let Some(ref v) = effects.class_hint {
        hooks.push(NarrativeHook {
            hook_type: HookType::Trait,
            source_scene: scene_id.to_string(),
            text: format!("Class: {}", v),
            mechanical_key: Some("class_hint".to_string()),
        });
    }

    if let Some(ref v) = effects.personality_trait {
        hooks.push(NarrativeHook {
            hook_type: HookType::Trait,
            source_scene: scene_id.to_string(),
            text: format!("Personality: {}", v),
            mechanical_key: Some("personality_trait".to_string()),
        });
    }

    if let Some(ref v) = effects.relationship {
        hooks.push(NarrativeHook {
            hook_type: HookType::Relationship,
            source_scene: scene_id.to_string(),
            text: format!("Relationship: {}", v),
            mechanical_key: Some("relationship".to_string()),
        });
    }

    if let Some(ref v) = effects.goals {
        hooks.push(NarrativeHook {
            hook_type: HookType::Goal,
            source_scene: scene_id.to_string(),
            text: format!("Goal: {}", v),
            mechanical_key: Some("goals".to_string()),
        });
    }

    if let Some(ref v) = effects.item_hint {
        hooks.push(NarrativeHook {
            hook_type: HookType::Possession,
            source_scene: scene_id.to_string(),
            text: format!("Item: {}", v),
            mechanical_key: Some("item_hint".to_string()),
        });
    }

    hooks
}

/// Convert a snake_case identifier to Title Case display name.
/// E.g. "natural_armor" → "Natural Armor", "mystery_compass" → "Mystery Compass".
fn humanize_snake_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let mut result = c.to_uppercase().to_string();
                    result.extend(chars);
                    result
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_anchors(scene_id: &str, effects: &MechanicalEffects) -> Vec<LoreAnchor> {
    let mut anchors = Vec::new();

    // Relationship effects can imply NPC anchors
    if let Some(ref v) = effects.relationship {
        anchors.push(LoreAnchor {
            anchor_type: "npc".to_string(),
            value: v.clone(),
            source_scene: scene_id.to_string(),
        });
    }

    anchors
}
