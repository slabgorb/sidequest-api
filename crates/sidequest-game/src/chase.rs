//! Chase state — escape threshold, round tracking, resolution.
//!
//! Implements ADR-017: three chase types, escape threshold (default 50%),
//! and round-by-round escape roll tracking.
//!
//! Extended with Chase Depth (C1-C5): rig damage, multi-actor roles,
//! beat system, terrain modifiers, and cinematography.

use serde::{Deserialize, Serialize};

use crate::chase_depth::{
    check_outcome, danger_for_beat, format_chase_context, phase_for_beat, terrain_modifiers,
    ChaseActor, ChaseBeat, ChaseOutcome, ChasePhase, RigStats, RigType,
};

/// The type of chase encounter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ChaseType {
    /// Physical pursuit.
    Footrace,
    /// Sneaking/hiding.
    Stealth,
    /// Talking your way out.
    Negotiation,
}

/// The result of a single chase round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaseRound {
    /// The player's escape roll (0.0 to 1.0).
    pub roll: f64,
    /// Whether the player escaped this round.
    pub escaped: bool,
}

/// Tracks the state of an active chase sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaseState {
    chase_type: ChaseType,
    escape_threshold: f64,
    round: u32,
    rounds: Vec<ChaseRound>,
    resolved: bool,
    /// Distance between pursuer and quarry (story 2-7).
    #[serde(default)]
    separation_distance: i32,
    /// Current chase phase description (story 2-7).
    #[serde(default)]
    chase_phase: Option<String>,
    /// Most recent chase event (story 2-7).
    #[serde(default)]
    chase_event: Option<String>,

    // -- Chase Depth (C1-C5) --

    /// Rig stats (C1). None for non-vehicle chases.
    #[serde(default)]
    rig: Option<RigStats>,
    /// Crew assignments (C2).
    #[serde(default)]
    actors: Vec<ChaseActor>,
    /// Current beat number (C3), 0-indexed.
    #[serde(default)]
    beat: u32,
    /// Separation goal for escape (C3).
    #[serde(default = "default_goal")]
    goal: i32,
    /// Current structured phase (C3).
    #[serde(default)]
    structured_phase: Option<ChasePhase>,
    /// Chase outcome if resolved (C3).
    #[serde(default)]
    outcome: Option<ChaseOutcome>,
}

fn default_goal() -> i32 {
    10
}

impl ChaseState {
    /// Create a new chase with the given type and escape threshold.
    pub fn new(chase_type: ChaseType, escape_threshold: f64) -> Self {
        Self {
            chase_type,
            escape_threshold,
            round: 1,
            rounds: Vec::new(),
            resolved: false,
            separation_distance: 0,
            chase_phase: None,
            chase_event: None,
            rig: None,
            actors: Vec::new(),
            beat: 0,
            goal: 10,
            structured_phase: None,
            outcome: None,
        }
    }

    /// Create a vehicle chase with rig stats (C1).
    pub fn new_vehicle_chase(
        chase_type: ChaseType,
        escape_threshold: f64,
        rig_type: RigType,
        goal: i32,
    ) -> Self {
        let mut state = Self::new(chase_type, escape_threshold);
        state.rig = Some(RigStats::from_type(rig_type));
        state.goal = goal;
        state.structured_phase = Some(ChasePhase::Setup);
        state
    }

    /// The type of this chase.
    pub fn chase_type(&self) -> ChaseType {
        self.chase_type
    }

    /// The escape threshold (roll must exceed this to escape).
    pub fn escape_threshold(&self) -> f64 {
        self.escape_threshold
    }

    /// Current round number.
    pub fn round(&self) -> u32 {
        self.round
    }

    /// The recorded chase rounds.
    pub fn rounds(&self) -> &[ChaseRound] {
        &self.rounds
    }

    /// Whether the chase has been resolved (escape or capture).
    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// Distance between pursuer and quarry.
    pub fn separation(&self) -> i32 {
        self.separation_distance
    }

    /// Set separation distance.
    pub fn set_separation(&mut self, distance: i32) {
        self.separation_distance = distance;
    }

    /// Current chase phase description.
    pub fn phase(&self) -> Option<&str> {
        self.chase_phase.as_deref()
    }

    /// Set chase phase.
    pub fn set_phase(&mut self, phase: String) {
        self.chase_phase = Some(phase);
    }

    /// Most recent chase event.
    pub fn event(&self) -> Option<&str> {
        self.chase_event.as_deref()
    }

    /// Set chase event.
    pub fn set_event(&mut self, event: String) {
        self.chase_event = Some(event);
    }

    /// Record an escape roll. Roll must strictly exceed threshold to escape.
    ///
    /// No-op if the chase is already resolved (escape or capture).
    pub fn record_roll(&mut self, roll: f64) {
        if self.resolved {
            return;
        }
        let escaped = roll > self.escape_threshold;
        self.rounds.push(ChaseRound { roll, escaped });
        self.round += 1;
        if escaped {
            self.resolved = true;
        }
    }

    // -- Chase Depth accessors (C1-C5) --

    /// Rig stats, if this is a vehicle chase (C1).
    pub fn rig(&self) -> Option<&RigStats> {
        self.rig.as_ref()
    }

    /// Mutable rig stats (C1).
    pub fn rig_mut(&mut self) -> Option<&mut RigStats> {
        self.rig.as_mut()
    }

    /// Set rig stats (C1).
    pub fn set_rig(&mut self, rig: RigStats) {
        self.rig = Some(rig);
    }

    /// Crew assignments (C2).
    pub fn actors(&self) -> &[ChaseActor] {
        &self.actors
    }

    /// Assign crew roles (C2).
    pub fn set_actors(&mut self, actors: Vec<ChaseActor>) {
        self.actors = actors;
    }

    /// Current beat number (C3).
    pub fn beat(&self) -> u32 {
        self.beat
    }

    /// Escape goal (C3).
    pub fn goal(&self) -> i32 {
        self.goal
    }

    /// Current structured phase (C3).
    pub fn structured_phase(&self) -> Option<ChasePhase> {
        self.structured_phase
    }

    /// Chase outcome (C3).
    pub fn outcome(&self) -> Option<ChaseOutcome> {
        self.outcome
    }

    /// Advance to the next beat (C3). Applies terrain damage (C4).
    /// Returns the new phase and any outcome.
    pub fn advance_beat(&mut self) -> (ChasePhase, Option<ChaseOutcome>) {
        self.beat += 1;
        let phase = phase_for_beat(self.beat, self.outcome.is_some());
        self.structured_phase = Some(phase);

        // Apply terrain damage (C4)
        let danger = danger_for_beat(self.beat, phase);
        let mods = terrain_modifiers(danger);
        if mods.rig_damage_per_beat > 0 {
            if let Some(ref mut rig) = self.rig {
                rig.apply_damage(mods.rig_damage_per_beat);
            }
        }

        // Check outcome (C3)
        let rig_hp = self.rig.as_ref().map_or(i32::MAX, |r| r.rig_hp);
        if let Some(outcome) = check_outcome(self.separation_distance, self.goal, rig_hp) {
            self.outcome = Some(outcome);
            self.resolved = true;
        }

        (phase, self.outcome)
    }

    /// Generate a ChaseBeat for the current state (C3).
    pub fn current_beat(&self, decisions: Vec<crate::chase_depth::BeatDecision>) -> ChaseBeat {
        let phase = self
            .structured_phase
            .unwrap_or_else(|| phase_for_beat(self.beat, self.outcome.is_some()));
        let danger = danger_for_beat(self.beat, phase);
        ChaseBeat {
            beat_number: self.beat,
            phase,
            decisions,
            terrain_danger: danger,
        }
    }

    /// Format narrator context for the current chase state (C1-C5).
    pub fn format_context(&self, decisions: Vec<crate::chase_depth::BeatDecision>) -> String {
        let beat = self.current_beat(decisions);
        let default_rig = RigStats::from_type(RigType::Frankenstein);
        let rig = self.rig.as_ref().unwrap_or(&default_rig);
        format_chase_context(&beat, rig, &self.actors, self.separation_distance, self.goal)
    }

    /// Mark the chase as abandoned (C3).
    pub fn abandon(&mut self) {
        self.outcome = Some(ChaseOutcome::Abandoned);
        self.resolved = true;
    }
}
