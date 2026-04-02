//! CharacterBuilder — state machine for genre-driven character creation.
//!
//! Story 2-3: Ports the Python CharacterBuilder as a typed state machine.
//! The builder doesn't exist before `new()` and is consumed conceptually by `build()`.
//! No IDLE or COMPLETE states — construction and consumption are the boundaries.

use std::collections::HashMap;

use sidequest_genre::{CharCreationScene, MechanicalEffects, RulesConfig};
use sidequest_protocol::{CharacterCreationPayload, CreationChoice, GameMessage, NonBlankString};

use crate::character::Character;
use crate::creature_core::CreatureCore;
use crate::inventory::{Inventory, Item};

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
}

impl CharacterBuilder {
    /// Create a new builder. Panics if `scenes` is empty.
    pub fn new(scenes: Vec<CharCreationScene>, rules: &RulesConfig) -> Self {
        assert!(
            !scenes.is_empty(),
            "CharacterBuilder requires at least one scene"
        );
        Self::build_inner(scenes, rules)
    }

    /// Create a new builder, returning an error if `scenes` is empty.
    pub fn try_new(
        scenes: Vec<CharCreationScene>,
        rules: &RulesConfig,
    ) -> Result<Self, BuilderError> {
        if scenes.is_empty() {
            return Err(BuilderError::NoScenes);
        }
        Ok(Self::build_inner(scenes, rules))
    }

    fn build_inner(scenes: Vec<CharCreationScene>, rules: &RulesConfig) -> Self {
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
            race_label: rules
                .race_label
                .clone()
                .unwrap_or_else(|| "Race".to_string()),
            class_label: rules
                .class_label
                .clone()
                .unwrap_or_else(|| "Class".to_string()),
        }
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
            // Collect the rich description text from each choice for backstory
            if let Some(ref desc) = result.choice_description {
                acc.backstory_fragments.push(desc.clone());
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
        if scene.allows_freeform != Some(true) {
            return Err(BuilderError::FreeformNotAllowed);
        }

        let effects = MechanicalEffects {
            class_hint: None,
            race_hint: None,
            mutation_hint: None,
            item_hint: None,
            affinity_hint: None,
            training_hint: None,
            background: None,
            personality_trait: None,
            emotional_state: None,
            relationship: None,
            goals: None,
            allows_freeform: None,
            rig_type_hint: None,
            rig_trait: None,
            catch_phrase: None,
            stat_bonuses: HashMap::new(),
            pronoun_hint: None,
        };

        self.results.push(SceneResult {
            input_type: SceneInputType::Freeform(text.to_string()),
            hooks_added: vec![],
            anchors_added: vec![],
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
        let stats = self.generate_stats(&acc);

        // HP from class base
        let base_hp = self
            .class_hp_bases
            .get(class_str)
            .copied()
            .or(self.default_hp)
            .unwrap_or(10) as i32;

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

        // Inventory from item hints
        let items: Vec<Item> = acc
            .item_hints
            .iter()
            .enumerate()
            .map(|(i, hint)| {
                let id_str = hint.to_lowercase().replace(' ', "_");
                Item {
                    id: NonBlankString::new(&id_str)
                        .unwrap_or_else(|_| NonBlankString::new(&format!("item_{}", i)).unwrap()),
                    name: NonBlankString::new(hint)
                        .unwrap_or_else(|_| NonBlankString::new("Unknown Item").unwrap()),
                    description: NonBlankString::new(&format!("Starting equipment: {}", hint))
                        .unwrap(),
                    category: NonBlankString::new("weapon").unwrap(),
                    value: 10,
                    weight: 3.0,
                    rarity: NonBlankString::new("common").unwrap(),
                    narrative_weight: 0.3,
                    tags: vec![],
                    equipped: true,
                    quantity: 1,
                }
            })
            .collect();

        // Compose backstory from the rich description text collected during
        // character creation. Each fragment is the flavor text the player saw
        // when they picked a choice (e.g. "A city built from stacked ruins…").
        // We weave them into prose rather than just listing mechanical labels.
        let backstory_text = if acc.backstory_fragments.is_empty() {
            // Fallback: use mechanical labels if somehow no descriptions were captured
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
        } else {
            acc.backstory_fragments.join(" ")
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

                GameMessage::CharacterCreation {
                    payload: CharacterCreationPayload {
                        phase: "scene".to_string(),
                        scene_index: Some(*scene_index as u32),
                        total_scenes: Some(self.scenes.len() as u32),
                        prompt: Some(scene.narration.clone()),
                        summary: None,
                        message: None,
                        choices: Some(choices),
                        allows_freeform: scene.allows_freeform,
                        input_type: Some("choice".to_string()),
                        loading_text: scene.loading_text.clone(),
                        character_preview: None,
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
                    choice: None,
                    character: None,
                },
                player_id: player_id.to_string(),
            },
            BuilderPhase::Confirmation => {
                let acc = self.accumulated();
                let mut parts = vec![
                    format!(
                        "{}: {}",
                        self.race_label,
                        acc.race_hint.as_deref().unwrap_or("Unknown")
                    ),
                    format!(
                        "{}: {}",
                        self.class_label,
                        acc.class_hint.as_deref().unwrap_or("Unknown")
                    ),
                    format!(
                        "Personality: {}",
                        acc.personality_trait.as_deref().unwrap_or("Unknown")
                    ),
                ];
                if let Some(ref m) = acc.mutation_hint {
                    parts.push(format!("Mutation: {}", m));
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
                    parts.push(format!("Equipment: {}", acc.item_hints.join(", ")));
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

    fn generate_stats(&self, acc: &AccumulatedChoices) -> HashMap<String, i32> {
        let base_values = match self.stat_generation.as_str() {
            "standard_array" => vec![15, 14, 13, 12, 10, 8],
            _ => vec![10; self.ability_score_names.len()],
        };

        let mut stats: HashMap<String, i32> = self
            .ability_score_names
            .iter()
            .zip(base_values.into_iter())
            .map(|(name, val)| (name.clone(), val))
            .collect();

        // Apply explicit stat bonuses from genre pack choices (origin, mutation, artifact)
        for (stat, bonus) in &acc.stat_bonuses {
            if let Some(val) = stats.get_mut(stat) {
                *val += bonus;
            }
        }

        // If no explicit bonuses were set and we're using the flat default,
        // derive differentiation from the player's accumulated choices so
        // stats aren't all 10.
        if acc.stat_bonuses.is_empty()
            && self.stat_generation != "standard_array"
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

        stats
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
