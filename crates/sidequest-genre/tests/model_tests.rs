//! Model deserialization tests for genre pack types.
//!
//! Tests in this file verify that each model struct:
//! - Deserializes correctly from representative YAML (matching real genre pack structure)
//! - Rejects unknown fields via `#[serde(deny_unknown_fields)]`
//! - Uses explicit types — no `serde_yaml::Value` catchalls
//!
//! These tests are derived from the actual `mutant_wasteland` genre pack YAML files.

use sidequest_genre::{
    // Core models — one per YAML file type
    Achievement,
    AssignmentMatrix,
    AtmosphereMatrix,
    AudioConfig,
    AxesConfig,
    BeatVocabulary,
    CartographyConfig,
    CharCreationScene,
    ClueGraph,
    Culture,
    GenreError,
    GenreTheme,
    Lore,
    NpcArchetype,
    PackMeta,
    PowerTier,
    ProgressionConfig,
    Prompts,
    RulesConfig,
    ScenarioPack,
    TropeDefinition,
    VisualStyle,
    WorldConfig,
    WorldLore,
};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// AC: Full model hierarchy — each YAML file type deserializes
// ═══════════════════════════════════════════════════════════

// ── PackMeta (pack.yaml) ─────────────────────────────────

#[test]
fn pack_meta_deserializes_from_yaml() {
    let yaml = r#"
name: Mutant Wasteland
version: "0.1.0"
description: Post-apocalyptic mutant adventure
min_sidequest_version: "0.1.0"
"#;
    let meta: PackMeta = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(meta.name.as_str(), "Mutant Wasteland");
    assert_eq!(meta.version, "0.1.0");
    assert_eq!(meta.min_sidequest_version, "0.1.0");
}

// ── RulesConfig (rules.yaml) ─────────────────────────────

#[test]
fn rules_config_deserializes_with_class_hp_bases() {
    let yaml = r#"
tone: gonzo-sincere
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names:
  - Brawn
  - Reflexes
  - Toughness
  - Wits
  - Instinct
  - Presence
allowed_classes:
  - Scavenger
  - Mutant
allowed_races:
  - Mutant Human
  - Pure Strain Human
class_hp_bases:
  Scavenger: 8
  Mutant: 10
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.ability_score_names.len(), 6);
    assert_eq!(rules.ability_score_names[0], "Brawn");
    assert_eq!(rules.allowed_classes.len(), 2);
    assert_eq!(*rules.class_hp_bases.get("Scavenger").unwrap(), 8u32);
    assert_eq!(rules.point_buy_budget, 27);
}

// ── NpcArchetype (archetypes.yaml) ───────────────────────

#[test]
fn npc_archetype_deserializes_with_stat_ranges() {
    let yaml = r#"
name: Wasteland Trader
description: A weathered merchant who travels between settlements
personality_traits:
  - shrewd
  - cheerfully cynical
typical_classes:
  - Scavenger
  - Tinker
typical_races:
  - Mutant Human
stat_ranges:
  Wits: [12, 16]
  Presence: [10, 14]
inventory_hints:
  - pack animal loaded with salvage
dialogue_quirks:
  - quotes prices in three different barter systems
disposition_default: 10
"#;
    let archetype: NpcArchetype = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(archetype.name.as_str(), "Wasteland Trader");
    assert_eq!(archetype.personality_traits.len(), 2);
    let wits_range = archetype.stat_ranges.get("Wits").unwrap();
    assert_eq!(wits_range, &[12, 16]);
    assert_eq!(archetype.disposition_default, 10);
}

// ── CharCreationScene (char_creation.yaml) ───────────────

#[test]
fn char_creation_scene_deserializes_with_choices() {
    let yaml = r#"
id: origins
title: Where You Woke Up
narration: The world ended a long time ago.
choices:
  - label: The Heaps
    description: A city built from stacked ruins
    mechanical_effects:
      class_hint: Scavenger
      race_hint: Mutant Human
  - label: The Overgrowth
    description: A forest that swallowed a city
    mechanical_effects:
      class_hint: Mutant
      race_hint: Plant Person
"#;
    let scene: CharCreationScene = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(scene.id, "origins");
    assert_eq!(scene.choices.len(), 2);
    assert_eq!(scene.choices[0].label, "The Heaps");
    assert_eq!(
        scene.choices[0].mechanical_effects.class_hint.as_deref(),
        Some("Scavenger")
    );
}

#[test]
fn char_creation_scene_allows_empty_choices() {
    // The confirmation scene in char_creation.yaml has `choices: []`
    let yaml = r#"
id: confirmation
title: The Wasteland Awaits
narration: You stand at the edge of the known.
choices: []
"#;
    let scene: CharCreationScene = serde_yaml::from_str(yaml).unwrap();
    assert!(scene.choices.is_empty());
}

// ── CharCreationScene mutation_hint variant ───────────────

#[test]
fn char_creation_choice_with_mutation_hint() {
    // The mutation scene uses mutation_hint instead of class_hint/race_hint
    let yaml = r#"
id: mutation
title: What Makes You Different
narration: In the wasteland, nobody looks the same.
choices:
  - label: Extra Limbs
    description: A third arm
    mechanical_effects:
      mutation_hint: extra_limbs
  - label: Nothing Visible
    description: You pass for normal
    mechanical_effects:
      mutation_hint: none
"#;
    let scene: CharCreationScene = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        scene.choices[0].mechanical_effects.mutation_hint.as_deref(),
        Some("extra_limbs")
    );
}

// ── TropeDefinition (tropes.yaml) ────────────────────────

#[test]
fn trope_definition_deserializes_with_escalation_and_passive_progression() {
    let yaml = r#"
id: inquisition_closes_in
name: The Inquisition Closes In
description: The Order tightens its grip
category: conflict
triggers:
  - use of magic
  - discovery of arcane items
narrative_hints:
  - inquisitor patrols appear on the road
tension_level: 0.7
resolution_hints:
  - expose the Veil's corruption
tags: [faction, persecution, magic]
escalation:
  - at: 0.2
    event: An Ashen Veil patrol stops the party
    npcs_involved: [Zealous Inquisitor]
    stakes: Any magical items could be confiscated
  - at: 0.8
    event: The Grand Inquisitor arrives
    npcs_involved: [Zealous Inquisitor]
    stakes: The noose tightens
passive_progression:
  rate_per_turn: 0.01
  rate_per_day: 0.02
  accelerators:
    - magic
    - spell
  decelerators:
    - hide
    - disguise
  accelerator_bonus: 0.05
  decelerator_penalty: 0.03
"#;
    let trope: TropeDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(trope.id.as_deref(), Some("inquisition_closes_in"));
    assert_eq!(trope.name.as_str(), "The Inquisition Closes In");
    assert_eq!(trope.category, "conflict");
    assert_eq!(trope.escalation.len(), 2);
    assert!((trope.escalation[0].at - 0.2).abs() < f64::EPSILON);
    assert_eq!(trope.tags, vec!["faction", "persecution", "magic"]);
    let pp = trope.passive_progression.as_ref().unwrap();
    assert!((pp.rate_per_turn - 0.01).abs() < f64::EPSILON);
}

#[test]
fn trope_definition_abstract_flag() {
    // Genre-level tropes in elemental_harmony use `abstract: true`
    let yaml = r#"
name: The Mentor
abstract: true
category: recurring
tags: [mentor, identity]
triggers:
  - player asks to be taught
narrative_hints:
  - lessons must emerge from stakes
resolution_patterns:
  - the student surpasses the mentor
tension_level: 0.5
"#;
    let trope: TropeDefinition = serde_yaml::from_str(yaml).unwrap();
    assert!(trope.is_abstract);
    assert_eq!(trope.name.as_str(), "The Mentor");
}

#[test]
fn trope_definition_extends_field() {
    // World tropes extend genre-level abstract tropes via `extends:`
    let yaml = r#"
name: The Wandering Blade
extends: the-mentor
description: A legendary swordsman walks the highways alone
triggers:
  - player encounters a martial arts school
narrative_hints:
  - the Wandering Blade appears at moments of crisis
tension_level: 0.6
resolution_hints:
  - the Blade offers a lesson, not a solution
tags:
  - rival
  - philosophy
escalation:
  - at: 0.2
    event: rumors of the Wandering Blade
    npcs_involved: [teahouse keeper]
    stakes: reputation
"#;
    let trope: TropeDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(trope.extends.as_deref(), Some("the-mentor"));
    assert_eq!(trope.name.as_str(), "The Wandering Blade");
}

// ── VisualStyle (visual_style.yaml) ──────────────────────

#[test]
fn visual_style_deserializes_with_tag_overrides() {
    let yaml = r#"
positive_suffix: post-apocalyptic digital painting
negative_prompt: clean, modern, pristine
preferred_model: flux
base_seed: 42
visual_tag_overrides:
  wasteland: cracked sun-baked earth
  settlement: ramshackle shelters
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(style.preferred_model, "flux");
    assert_eq!(style.base_seed, 42);
    assert!(style.visual_tag_overrides.contains_key("wasteland"));
    assert_eq!(style.visual_tag_overrides.len(), 2);
}

// ── ProgressionConfig (progression.yaml) ─────────────────

#[test]
fn progression_config_deserializes_with_affinities_and_wealth_tiers() {
    // Tests deeply nested structure and null max_gold (no cap on wealth)
    let yaml = r#"
affinities:
  - name: Flux
    description: Mutation control
    triggers:
      - using a mutation deliberately
    tier_thresholds: [5, 12, 25]
    unlocks:
      tier_1:
        name: Shifting
        description: Your body has started to change on purpose
        abilities:
          - name: Forced Growth
            experience: It starts as an itch deep under the skin
            limits: Growth that isn't guided goes wrong
milestone_categories:
  - survival
  - salvage
milestones_per_level: 3
max_level: 10
item_evolution:
  naming_threshold: 0.5
  power_up_threshold: 0.8
level_bonuses:
  stat_points: 1
  hp_bonus: class_based
wealth_tiers:
  - max_gold: 0
    label: starving
  - max_gold: 10
    label: scraping
  - max_gold: null
    label: pre-war rich
"#;
    let prog: ProgressionConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(prog.affinities.len(), 1);
    assert_eq!(prog.affinities[0].name, "Flux");
    assert_eq!(prog.affinities[0].tier_thresholds, vec![5, 12, 25]);
    assert_eq!(prog.max_level, 10);
    assert_eq!(prog.milestones_per_level, 3);
    // wealth_tiers: last tier has max_gold: null (no cap)
    let last_tier = prog.wealth_tiers.last().unwrap();
    assert!(
        last_tier.max_gold.is_none(),
        "null max_gold should deserialize as None"
    );
    assert_eq!(last_tier.label, "pre-war rich");
    // First tier has max_gold: 0
    assert_eq!(prog.wealth_tiers[0].max_gold, Some(0));
}

// ── AxesConfig (axes.yaml) ───────────────────────────────

#[test]
fn axes_config_deserializes_with_definitions_and_presets() {
    let yaml = r#"
definitions:
  - id: weirdness
    name: Weirdness
    description: How strange the world gets
    poles:
      - grounded
      - gonzo
    default: 0.6
  - id: tech_level
    name: Tech Level
    description: How much Ancient tech is around
    poles:
      - scrapyard
      - functional
    default: 0.4
modifiers:
  weirdness:
    grounded: Keep it realistic
    gonzo: Embrace the weird
presets:
  - name: Caves of Qud
    description: Maximum weirdness
    values:
      weirdness: 0.9
      tech_level: 0.5
      hope: 0.6
"#;
    let axes: AxesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(axes.definitions.len(), 2);
    assert_eq!(axes.definitions[0].id, "weirdness");
    assert!((axes.definitions[0].default - 0.6).abs() < f64::EPSILON);
    assert_eq!(axes.definitions[0].poles, vec!["grounded", "gonzo"]);
    assert_eq!(axes.presets.len(), 1);
    assert_eq!(axes.presets[0].name, "Caves of Qud");
    let qud_weirdness = axes.presets[0].values.get("weirdness").unwrap();
    assert!((*qud_weirdness - 0.9).abs() < f64::EPSILON);
}

// ── AudioConfig (audio.yaml) ─────────────────────────────

#[test]
fn audio_config_deserializes_with_mood_tracks_and_mixer() {
    let yaml = r#"
mood_tracks:
  exploration:
    - path: audio/music/exploration.ogg
      title: Wasteland Horizon
      bpm: 90
  combat:
    - path: audio/music/combat.ogg
      title: Scrap Iron Fury
      bpm: 140
sfx_library:
  explosion:
    - audio/sfx/explosion.ogg
    - audio/sfx/explosion_2.ogg
creature_voice_presets:
  mutant_brute:
    creature_type: mutant_brute
    description: Deep guttural voice
    pitch: 0.5
    rate: 0.7
    effects:
      - type: reverb
        params:
          room_size: 0.5
mixer:
  music_volume: 0.4
  sfx_volume: 0.7
  voice_volume: 1.0
  duck_music_for_voice: true
  duck_amount_db: -12.0
  crossfade_default_ms: 3000
themes:
  - name: exploration
    mood: exploration
    base_prompt: ""
    variations:
      - type: full
        path: audio/music/set-1/exploration.ogg
"#;
    let audio: AudioConfig = serde_yaml::from_str(yaml).unwrap();
    let exploration_tracks = audio.mood_tracks.get("exploration").unwrap();
    assert_eq!(exploration_tracks[0].title, "Wasteland Horizon");
    assert_eq!(exploration_tracks[0].bpm, 90);
    assert!(audio.mixer.duck_music_for_voice);
    assert_eq!(audio.themes.len(), 1);
    let brute = audio.creature_voice_presets.get("mutant_brute").unwrap();
    assert!((brute.pitch - 0.5).abs() < f64::EPSILON);
}

// ── CartographyConfig (cartography.yaml) ─────────────────

#[test]
fn cartography_config_deserializes_with_regions_and_routes() {
    let yaml = r#"
world_name: The Flickering Reach
starting_region: toods_dome
map_style: Military tactical display
map_resolution: null
regions:
  toods_dome:
    name: Tood's Dome
    description: A Scrapborn trade-city
    adjacent:
      - glass_flat
      - blooming_tangle
    landmarks: []
    origin: null
    rivers: []
    settlements: []
  glass_flat:
    name: The Glass Flat
    description: 40-mile-wide plain of fused black glass
    adjacent:
      - toods_dome
    landmarks: []
    origin: null
    rivers: []
    settlements: []
routes:
  - name: The Burn Line
    from_id: toods_dome
    to_id: blooming_tangle
    distance: short
    danger: moderate
    description: A contested strip
"#;
    let carto: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(carto.starting_region, "toods_dome");
    assert!(
        carto.map_resolution.is_none(),
        "null map_resolution should be None"
    );
    let dome = carto.regions.get("toods_dome").unwrap();
    assert_eq!(dome.name, "Tood's Dome");
    assert!(dome.adjacent.contains(&"glass_flat".to_string()));
    assert_eq!(carto.routes.len(), 1);
    assert_eq!(carto.routes[0].danger, "moderate");
}

// ── WorldConfig (world.yaml) ─────────────────────────────

#[test]
fn world_config_deserializes_with_axis_snapshot() {
    let yaml = r#"
name: The Flickering Reach
slug: flickering_reach
description: Three wounds define the Flickering Reach
starting_location: the trade post at the edge of the black glass plain
axis_snapshot:
  hope: 0.6
  tech_level: 0.5
  weirdness: 0.9
"#;
    let world: WorldConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(world.slug, "flickering_reach");
    let weirdness = world.axis_snapshot.get("weirdness").unwrap();
    assert!((*weirdness - 0.9).abs() < f64::EPSILON);
}

// ── Culture (cultures.yaml) ──────────────────────────────

#[test]
fn culture_deserializes_with_corpora_and_word_list_slots() {
    // Slots have two variants: corpora+lookback OR word_list
    let yaml = r#"
name: Scrapborn
description: Urban ruin-dwellers who build from salvage
slots:
  given_name:
    corpora:
      - corpus: english.txt
        weight: 1.0
    lookback: 2
  clan_name:
    word_list: [Voltkin, Rustblood, Wirebound]
person_patterns:
  - "{given_name} {clan_name}"
place_patterns:
  - "The {adjective} {place_noun}"
"#;
    let culture: Culture = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(culture.name.as_str(), "Scrapborn");
    assert!(culture.slots.contains_key("given_name"));
    assert!(culture.slots.contains_key("clan_name"));
    assert_eq!(culture.person_patterns.len(), 1);
    // word_list slot has no corpora
    let clan = culture.slots.get("clan_name").unwrap();
    assert!(clan.word_list.is_some());
    assert!(clan.corpora.is_none());
    // corpora slot has no word_list
    let given = culture.slots.get("given_name").unwrap();
    assert!(given.corpora.is_some());
    assert_eq!(given.lookback, Some(2));
}

// ── GenreTheme (theme.yaml) ──────────────────────────────

#[test]
fn genre_theme_deserializes_with_dinkus() {
    let yaml = r#"
primary: '#4a7c2e'
secondary: '#8b4513'
accent: '#ff6600'
background: '#1a1a0e'
surface: '#2a2a1a'
text: '#c8d6a0'
border_style: heavy
web_font_family: Special Elite
dinkus:
  enabled: true
  cooldown: 2
  default_weight: medium
  glyph:
    light: "·  ☢  ·"
    medium: "☢  ☢  ☢"
    heavy: "☢ ☣ ☢"
session_opener:
  enabled: true
"#;
    let theme: GenreTheme = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(theme.primary, "#4a7c2e");
    assert_eq!(theme.border_style, "heavy");
    assert!(theme.dinkus.enabled);
    assert_eq!(theme.dinkus.glyph.get("heavy").unwrap(), "☢ ☣ ☢");
}

// ── Lore (lore.yaml) ─────────────────────────────────────

#[test]
fn lore_deserializes_with_optional_empty_strings() {
    // Genre-level lore.yaml has empty world_name and geography
    let yaml = r#"
world_name: ""
history: The world ended.
geography: ""
cosmology: The Ancients built wonders.
"#;
    let lore: Lore = serde_yaml::from_str(yaml).unwrap();
    assert!(lore.world_name.is_empty());
    assert!(!lore.history.is_empty());
    assert!(lore.geography.is_empty());
}

// ── WorldLore (worlds/*/lore.yaml) ───────────────────────

#[test]
fn world_lore_deserializes_with_factions() {
    let yaml = r#"
world_name: The Flickering Reach
history: Three wounds define the Reach.
geography: The Flickering Reach sprawls.
cosmology: The people do not agree on gods.
factions:
  - name: The Dome Syndicate
    description: Scrapborn trade coalition
    disposition: neutral
  - name: The Sealed Protocol
    description: Vaultborn isolationist government
    disposition: hostile
"#;
    let lore: WorldLore = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(lore.factions.len(), 2);
    assert_eq!(lore.factions[0].disposition, "neutral");
    assert_eq!(lore.factions[1].disposition, "hostile");
}

// ── Prompts (prompts.yaml) ───────────────────────────────

#[test]
fn prompts_deserializes_with_transition_hints() {
    let yaml = r#"
narrator: You are the narrator of a post-apocalyptic world
combat: Combat in the wasteland is scrappy
npc: NPCs in the wasteland are survivors
world_state: Track the state of the wasteland
transition_hints:
  smash_cut: Static burst. Begin mid-action.
  dissolve: Radiation shimmer blurs the edges.
"#;
    let prompts: Prompts = serde_yaml::from_str(yaml).unwrap();
    assert!(!prompts.narrator.is_empty());
    assert!(prompts.transition_hints.contains_key("smash_cut"));
    assert!(prompts.transition_hints.contains_key("dissolve"));
}

// ── BeatVocabulary (beat_vocabulary.yaml) ─────────────────

#[test]
fn beat_vocabulary_deserializes_obstacles() {
    let yaml = r#"
obstacles:
  - name: Fallen tree
    description: A massive trunk blocks the forest trail
    stat_check: Athletics
    failure_penalty: Stamina -1
    tags: [forest, natural]
  - name: Market crowd
    description: Dense foot traffic
    stat_check: Dexterity
    failure_penalty: Separation -1
    tags: [urban, crowd]
"#;
    let vocab: BeatVocabulary = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(vocab.obstacles.len(), 2);
    assert_eq!(vocab.obstacles[0].stat_check, "Athletics");
    assert_eq!(vocab.obstacles[1].tags, vec!["urban", "crowd"]);
}

// ── Achievement (achievements.yaml) ──────────────────────

#[test]
fn achievement_deserializes() {
    let yaml = r#"
id: inquisition_awakened
name: Marked by the Veil
description: The Inquisition has taken notice
trope_id: inquisition_closes_in
trigger_status: activated
emoji: "\U0001F525"
"#;
    let achievement: Achievement = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(achievement.id, "inquisition_awakened");
    assert_eq!(achievement.trope_id, "inquisition_closes_in");
    assert_eq!(achievement.trigger_status, "activated");
}

// ── PowerTier (power_tiers.yaml) ─────────────────────────

#[test]
fn power_tier_deserializes_with_level_range() {
    // power_tiers.yaml is keyed by class name, each value is Vec<PowerTier>
    let yaml = r#"
- level_range: [1, 3]
  label: picker
  player: threadbare scavenged clothing
  npc: wasteland junk picker
- level_range: [4, 6]
  label: scrounger
  player: functional scrap armor
  npc: settlement trade scavenger
"#;
    let tiers: Vec<PowerTier> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(tiers.len(), 2);
    assert_eq!(tiers[0].level_range, [1, 3]);
    assert_eq!(tiers[0].label, "picker");
    assert_eq!(tiers[1].level_range, [4, 6]);
}

// ═══════════════════════════════════════════════════════════
// AC: deny_unknown_fields — YAML typos produce clear errors
// ═══════════════════════════════════════════════════════════

#[test]
fn pack_meta_rejects_unknown_field() {
    let yaml = r#"
name: Test Pack
version: "0.1.0"
description: Test
min_sidequest_version: "0.1.0"
bogus_field: oops
"#;
    let result: Result<PackMeta, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject bogus_field"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("bogus_field") || err_msg.contains("unknown field"),
        "error should mention the unknown field, got: {err_msg}"
    );
}

#[test]
fn rules_config_rejects_typo() {
    let yaml = r#"
tone: test
lethality: low
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: []
allowed_classes: []
allowed_races: []
class_hp_bases: {}
letality: this-is-a-typo
"#;
    let result: Result<RulesConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject 'letality' typo"
    );
}

#[test]
fn npc_archetype_rejects_unknown_field() {
    let yaml = r#"
name: Test NPC
description: Test
personality_traits: []
typical_classes: []
typical_races: []
stat_ranges: {}
inventory_hints: []
dialogue_quirks: []
disposition_default: 0
secret_power: should_not_exist
"#;
    let result: Result<NpcArchetype, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject secret_power"
    );
}

#[test]
fn visual_style_rejects_unknown_field() {
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: flux
base_seed: 42
visual_tag_overrides: {}
unknown_setting: bad
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject unknown_setting"
    );
}

#[test]
fn trope_definition_rejects_unknown_field() {
    let yaml = r#"
name: Test Trope
category: conflict
triggers: []
narrative_hints: []
tension_level: 0.5
secret_mechanic: should_fail
"#;
    let result: Result<TropeDefinition, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject secret_mechanic"
    );
}

// ═══════════════════════════════════════════════════════════
// Rule enforcement: #[non_exhaustive] on error enums (Rule #2)
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_error_is_non_exhaustive() {
    // GenreError must be #[non_exhaustive] so downstream crates
    // use a wildcard arm when matching. This test verifies the
    // type exists and has expected variants.
    let err = GenreError::LoadError {
        path: "test.yaml".into(),
        detail: "test error".into(),
    };
    // Must use wildcard because #[non_exhaustive]
    match err {
        GenreError::LoadError { .. } => {}
        _ => panic!("unexpected variant — wildcard required by non_exhaustive"),
    }
}

// ═══════════════════════════════════════════════════════════
// Rule enforcement: no serde_yaml::Value catchalls (AC: no untyped)
// ═══════════════════════════════════════════════════════════

#[test]
fn progression_wealth_tier_max_gold_is_typed_not_value() {
    // Python used dict[str, Any] for level_bonuses. Rust must use explicit types.
    // max_gold: null in YAML → Option<u32>, not serde_yaml::Value
    let yaml = r#"
max_gold: null
label: pre-war rich
"#;
    let tier: sidequest_genre::WealthTier = serde_yaml::from_str(yaml).unwrap();
    assert!(tier.max_gold.is_none());

    let yaml_with_value = r#"
max_gold: 1000
label: connected
"#;
    let tier: sidequest_genre::WealthTier = serde_yaml::from_str(yaml_with_value).unwrap();
    assert_eq!(tier.max_gold, Some(1000));
}

// ═══════════════════════════════════════════════════════════
// AC: Scenario pack models (from pulp_noir prototype)
// ═══════════════════════════════════════════════════════════

// ── ScenarioPack (scenario.yaml) ─────────────────────────

#[test]
fn scenario_pack_deserializes_with_player_roles_and_pacing() {
    let yaml = r#"
name: Murder on the Midnight Express
version: "1.0.0"
description: A locked-room mystery aboard the Orient Express
duration_minutes: 210
max_players: 5
player_roles:
  - id: lead_investigator
    archetype_hint: A retired detective
    narrative_position: You were asked to investigate
    required_hooks:
      - type: MOTIVATION
        prompt: What compels you to seek the truth?
    constraints:
      - Must have a plausible reason to investigate
    suggested_flavors:
      - Retired military intelligence
pacing:
  scene_budget: 16
  acts:
    - id: act_1
      name: Discovery
      scenes: 5
      trope_range: [0.0, 0.35]
      narrator_tone: Atmospheric dread
  pressure_events:
    - at_scene: 5
      event: A scream echoes from the luggage van
  escalation_beats:
    - at: 0.50
      inject: Alibis are crumbling
"#;
    let scenario: ScenarioPack = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(scenario.name.as_str(), "Murder on the Midnight Express");
    assert_eq!(scenario.max_players, 5);
    assert_eq!(scenario.duration_minutes, 210);
    assert_eq!(scenario.player_roles.len(), 1);
    assert_eq!(scenario.player_roles[0].id, "lead_investigator");
    assert_eq!(scenario.pacing.scene_budget, 16);
    assert_eq!(scenario.pacing.acts[0].name, "Discovery");
}

// ── AssignmentMatrix (assignment_matrix.yaml) ────────────

#[test]
fn assignment_matrix_deserializes_with_suspects() {
    let yaml = r#"
suspects:
  - id: suspect_varek
    archetype_ref: train_conductor
    can_be_guilty: true
    motives: [revenge, greed]
    methods: [poison, staged_accident]
    opportunities: [dining_hall, corridor]
  - id: suspect_irina
    archetype_ref: countess
    can_be_guilty: false
    motives: [jealousy]
    methods: [poison]
    opportunities: [dining_hall]
motives:
  - betrayal
  - revenge
  - greed
methods:
  - poison
  - suffocation
  - stabbing
opportunities:
  - locked_car
  - dining_hall
"#;
    let matrix: AssignmentMatrix = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(matrix.suspects.len(), 2);
    assert!(matrix.suspects[0].can_be_guilty);
    assert!(!matrix.suspects[1].can_be_guilty);
    assert_eq!(matrix.suspects[0].motives, vec!["revenge", "greed"]);
    assert_eq!(matrix.motives.len(), 3);
}

// ── ClueGraph (clue_graph.yaml) ──────────────────────────

#[test]
fn clue_graph_deserializes_with_nodes() {
    let yaml = r#"
nodes:
  - id: clue_poison_vial
    type: physical
    description: A small glass vial with traces of belladonna
    discovery_method: forensic
    visibility: hidden
    locations: [dining_car, luggage_van]
    implicates: [suspect_varek]
    requires: []
    red_herring: false
  - id: clue_corridor_witness
    type: testimonial
    description: A sleepless passenger saw a figure
    discovery_method: interrogate
    visibility: obvious
    locations: []
    implicates: [suspect_lucienne]
    requires: []
    red_herring: false
"#;
    let graph: ClueGraph = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.nodes[0].id, "clue_poison_vial");
    assert!(!graph.nodes[0].red_herring);
    assert_eq!(graph.nodes[0].implicates, vec!["suspect_varek"]);
    assert_eq!(graph.nodes[0].visibility, "hidden");
}

// ── AtmosphereMatrix (atmosphere_matrix.yaml) ────────────

#[test]
fn atmosphere_matrix_deserializes_with_null_concurrent_event() {
    let yaml = r#"
variants:
  - id: stormy_crossing
    weather: Driving rain, thunder
    setting_status: doors_locked
    mood_baseline: Claustrophobic dread
    concurrent_event: Lightning strikes the rail bridge
    npc_mood_overrides:
      suspect_varek: visibly sweating
  - id: midnight_passage
    weather: Clear and bitterly cold
    setting_status: lights_dimmed
    mood_baseline: Paranoid stillness
    concurrent_event: null
    npc_mood_overrides:
      suspect_rashid: unusually talkative
"#;
    let atmo: AtmosphereMatrix = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(atmo.variants.len(), 2);
    assert!(atmo.variants[0].concurrent_event.is_some());
    assert!(
        atmo.variants[1].concurrent_event.is_none(),
        "null concurrent_event should be None"
    );
    assert!(atmo.variants[0]
        .npc_mood_overrides
        .contains_key("suspect_varek"));
}
