//! Scenario pack types from `scenarios/*/`.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

/// A scenario pack — assembled from scenario.yaml + supporting files.
///
/// Fields from scenario.yaml are required; fields from supplementary files
/// (assignment_matrix, clue_graph, etc.) default to empty and are populated
/// by the loader after reading the additional YAML files.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPack {
    /// Scenario display name.
    pub name: NonBlankString,
    /// Semantic version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Expected play time in minutes.
    pub duration_minutes: u32,
    /// Maximum number of players.
    pub max_players: u32,
    /// Available player roles.
    pub player_roles: Vec<PlayerRole>,
    /// Pacing and act structure.
    pub pacing: Pacing,
    /// Suspect/motive/method assignment matrix.
    #[serde(default)]
    pub assignment_matrix: AssignmentMatrix,
    /// Clue dependency graph.
    #[serde(default)]
    pub clue_graph: ClueGraph,
    /// Atmosphere/weather variants.
    #[serde(default)]
    pub atmosphere_matrix: AtmosphereMatrix,
    /// NPC definitions with guilty/innocent branches.
    #[serde(default)]
    pub npcs: Vec<ScenarioNpc>,
}

/// A player role within a scenario.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRole {
    /// Role identifier.
    pub id: String,
    /// Suggested archetype description.
    pub archetype_hint: String,
    /// Narrative position text.
    pub narrative_position: String,
    /// Required character hooks.
    #[serde(default)]
    pub required_hooks: Vec<RoleHook>,
    /// Constraints on this role.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Flavor text suggestions.
    #[serde(default)]
    pub suggested_flavors: Vec<String>,
}

/// A required hook for a player role.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleHook {
    /// Hook type (MOTIVATION, RELATIONSHIP, etc.).
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Prompt question.
    pub prompt: String,
}

/// Scenario pacing and act structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pacing {
    /// Total scene budget.
    pub scene_budget: u32,
    /// Act definitions.
    pub acts: Vec<Act>,
    /// Pressure events triggered at specific scenes.
    #[serde(default)]
    pub pressure_events: Vec<PressureEvent>,
    /// Escalation beats at progression thresholds.
    #[serde(default)]
    pub escalation_beats: Vec<EscalationBeat>,
}

/// An act within a scenario's pacing structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Act {
    /// Act identifier.
    pub id: String,
    /// Act name.
    pub name: String,
    /// Number of scenes in this act.
    pub scenes: u32,
    /// Trope progression range [start, end].
    pub trope_range: [f64; 2],
    /// Narrator tone guidance.
    pub narrator_tone: String,
}

/// A pressure event triggered at a specific scene.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PressureEvent {
    /// Scene number that triggers this event.
    pub at_scene: u32,
    /// Event description.
    pub event: String,
}

/// An escalation beat at a progression threshold.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EscalationBeat {
    /// Progression threshold (0.0–1.0).
    pub at: f64,
    /// Injected narrative text.
    pub inject: String,
}

/// Suspect/motive/method assignment matrix.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentMatrix {
    /// Suspect definitions.
    #[serde(default)]
    pub suspects: Vec<Suspect>,
    /// Available motives.
    #[serde(default)]
    pub motives: Vec<String>,
    /// Available methods.
    #[serde(default)]
    pub methods: Vec<String>,
    /// Available opportunities.
    #[serde(default)]
    pub opportunities: Vec<String>,
}

/// A suspect in the assignment matrix.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Suspect {
    /// Suspect identifier.
    pub id: String,
    /// Reference to an NPC archetype.
    pub archetype_ref: String,
    /// Whether this suspect can be the guilty party.
    pub can_be_guilty: bool,
    /// Possible motives for this suspect.
    pub motives: Vec<String>,
    /// Possible methods for this suspect.
    pub methods: Vec<String>,
    /// Possible opportunities for this suspect.
    pub opportunities: Vec<String>,
}

/// Clue dependency graph.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ClueGraph {
    /// Clue nodes.
    #[serde(default)]
    pub nodes: Vec<ClueNode>,
}

/// A single clue node in the graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClueNode {
    /// Clue identifier.
    pub id: String,
    /// Clue type (physical, testimonial, behavioral).
    #[serde(rename = "type")]
    pub clue_type: String,
    /// Description.
    pub description: String,
    /// How the clue is discovered.
    pub discovery_method: String,
    /// Visibility level.
    pub visibility: String,
    /// Locations where this clue can be found.
    #[serde(default)]
    pub locations: Vec<String>,
    /// Suspect IDs this clue implicates.
    #[serde(default)]
    pub implicates: Vec<String>,
    /// Prerequisite clue IDs.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Whether this is a red herring.
    #[serde(default)]
    pub red_herring: bool,
}

/// Atmosphere variant matrix.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereMatrix {
    /// Atmosphere variants.
    #[serde(default)]
    pub variants: Vec<AtmosphereVariant>,
}

/// A single atmosphere variant.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereVariant {
    /// Variant identifier.
    pub id: String,
    /// Weather description.
    pub weather: String,
    /// Setting status (doors_locked, lights_dimmed, normal).
    pub setting_status: String,
    /// Mood baseline description.
    pub mood_baseline: String,
    /// Concurrent event (null if none).
    pub concurrent_event: Option<String>,
    /// Per-NPC mood overrides.
    #[serde(default)]
    pub npc_mood_overrides: HashMap<String, String>,
}

/// An NPC within a scenario with branching behavior.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioNpc {
    /// NPC identifier.
    pub id: String,
    /// Reference to an archetype.
    pub archetype_ref: String,
    /// Display name.
    pub name: String,
    /// Starting beliefs and knowledge.
    pub initial_beliefs: InitialBeliefs,
    /// Behavior when this NPC is the guilty party.
    pub when_guilty: WhenGuilty,
    /// Behavior when this NPC is innocent.
    pub when_innocent: WhenInnocent,
}

/// An NPC's initial beliefs and suspicions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitialBeliefs {
    /// Known facts.
    #[serde(default)]
    pub facts: Vec<String>,
    /// Suspicions about other suspects.
    #[serde(default)]
    pub suspicions: Vec<Suspicion>,
}

/// A suspicion one NPC has about another.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Suspicion {
    /// Target suspect ID.
    pub target: String,
    /// Confidence level (0.0–1.0).
    pub confidence: f64,
    /// Basis for the suspicion.
    pub basis: String,
}

/// NPC behavior when guilty.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhenGuilty {
    /// What actually happened.
    pub truth: String,
    /// The NPC's false alibi.
    pub cover_story: String,
    /// Clue IDs that break the cover story.
    #[serde(default)]
    pub breaking_evidence: Vec<String>,
}

/// NPC behavior when innocent.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhenInnocent {
    /// What the NPC was actually doing.
    pub actual_activity: String,
    /// Who the NPC suspects.
    #[serde(default)]
    pub suspicion: String,
    /// The NPC's secret (unrelated to the crime).
    #[serde(default)]
    pub secret: String,
}
