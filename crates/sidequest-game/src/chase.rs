//! Chase state — escape threshold, round tracking, resolution.
//!
//! Implements ADR-017: three chase types, escape threshold (default 50%),
//! and round-by-round escape roll tracking.

use serde::{Deserialize, Serialize};

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
        }
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
}
