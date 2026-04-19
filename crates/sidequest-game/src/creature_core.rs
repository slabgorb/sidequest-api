//! CreatureCore — shared fields and behavior for Character and NPC.
//!
//! Story 1-13: Extracted from Character and NPC to eliminate duplication.
//! Both types embed `CreatureCore` via composition.
//!
//! Epic 39 — Edge / Composure Combat:
//!   * Story 39-1 introduced the `EdgePool` type family.
//!   * Story 39-2 removed the legacy `hp/max_hp/ac` fields from this
//!     struct and added `edge: EdgePool` + `acquired_advancements:
//!     Vec<String>`. Constructors synthesize a placeholder `EdgePool`
//!     with `base_max == 10`; 39-3 tunes per-class via YAML.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::combatant::Combatant;
use crate::inventory::Inventory;
use crate::thresholds::{detect_crossings, ThresholdAt};

/// Default placeholder base_max for edge pools synthesized by constructors
/// that haven't yet been wired to per-class YAML (story 39-3).
pub const PLACEHOLDER_EDGE_BASE_MAX: i32 = 10;

/// Build a placeholder `EdgePool` for constructors that do not yet have
/// per-class edge tuning (story 39-3 replaces individual call-sites with
/// YAML-driven values). The pool starts at full composure, carries
/// `OnResolution` as the default recovery trigger, and has no authored
/// thresholds — those are content in 39-6.
pub fn placeholder_edge_pool() -> EdgePool {
    EdgePool {
        current: PLACEHOLDER_EDGE_BASE_MAX,
        max: PLACEHOLDER_EDGE_BASE_MAX,
        base_max: PLACEHOLDER_EDGE_BASE_MAX,
        recovery_triggers: vec![RecoveryTrigger::OnResolution],
        thresholds: vec![],
    }
}

/// Shared fields for any creature (Character or NPC).
///
/// Embedded via composition with `#[serde(flatten)]` so JSON
/// serialization remains unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatureCore {
    /// Creature's display name.
    pub name: NonBlankString,
    /// Physical description.
    pub description: NonBlankString,
    /// Personality traits and mannerisms.
    pub personality: NonBlankString,
    /// Creature level (1+).
    pub level: u32,
    /// Experience points accumulated.
    #[serde(default)]
    pub xp: u32,
    /// Inventory of carried items.
    pub inventory: Inventory,
    /// Active status conditions.
    pub statuses: Vec<String>,
    /// Composure pool — the HP analogue in epic 39. Stories 39-3/4/6
    /// tune thresholds, recovery triggers, and per-class base_max.
    pub edge: EdgePool,
    /// Advancement ids the creature has acquired (epic 39 mechanical
    /// progression). Populated by 39-8; stays empty at construction.
    #[serde(default)]
    pub acquired_advancements: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// Epic 39 — EdgePool composure currency (story 39-1)
// ═══════════════════════════════════════════════════════════════════════

/// A downward threshold on an `EdgePool`.
///
/// Mirrors `ResourceThreshold`, but typed for i32 composure values so
/// edge scenes don't have to round-trip through f64.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeThreshold {
    /// Value at which this threshold fires (crossed downward).
    pub at: i32,
    /// Event identifier emitted when crossed (e.g. `edge_strained`).
    pub event_id: String,
    /// Narrator hint injected into prompt when crossed.
    pub narrator_hint: String,
}

impl ThresholdAt for EdgeThreshold {
    type Value = i32;

    fn at(&self) -> i32 {
        self.at
    }
    fn event_id(&self) -> &str {
        &self.event_id
    }
    fn narrator_hint(&self) -> &str {
        &self.narrator_hint
    }
}

/// Trigger that grants composure back to an `EdgePool`.
///
/// Authored in genre YAML (39-6) and resolved during beat dispatch
/// (39-4). Story 39-1 only introduces the shape; no engine wiring yet.
///
/// Marked `#[non_exhaustive]` because genre authors are expected to
/// add further recovery variants in later stories — keeps external
/// `match` arms honest about the open set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecoveryTrigger {
    /// Restore edge when the encounter resolves (win or escape).
    OnResolution,
    /// An ally spending an action to shore up the creature.
    OnAllyRescue,
    /// A specific authored beat landing, optionally gated on
    /// the creature being strained (`current <= max / 4`).
    OnBeatSuccess {
        /// Beat identifier that triggers the recovery.
        beat_id: String,
        /// How much edge to restore.
        amount: i32,
        /// If true, only fires while the pool is in the strained band.
        while_strained: bool,
    },
}

/// Result of applying a delta to an `EdgePool`.
///
/// Mirrors the crossed-threshold shape of `ResourcePatchResult` but is
/// i32-valued and carries the threshold type specific to EdgePool.
#[derive(Debug, Clone, PartialEq)]
pub struct DeltaResult {
    /// Pool value after the delta was applied (post-clamp).
    pub new_current: i32,
    /// Thresholds crossed by this delta (downward only).
    pub crossed: Vec<EdgeThreshold>,
}

/// First-class composure pool for a creature (epic 39).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePool {
    /// Current composure value (clamped to `[0, max]`).
    pub current: i32,
    /// Current maximum composure (may be reduced mid-scene).
    pub max: i32,
    /// Baseline max — the starting ceiling, used when recovery
    /// triggers restore `max` back up after temporary reductions.
    pub base_max: i32,
    /// Triggers that refill edge during play.
    pub recovery_triggers: Vec<RecoveryTrigger>,
    /// Downward thresholds that fire named events when crossed.
    pub thresholds: Vec<EdgeThreshold>,
}

impl EdgePool {
    /// Apply a composure delta and detect threshold crossings.
    ///
    /// Positive delta increases `current` (capped at `max`). Negative
    /// delta decreases `current` (floored at 0 — composure never goes
    /// negative; a creature at 0 is broken). Returns the new current
    /// plus any thresholds crossed downward by this delta.
    pub fn apply_delta(&mut self, delta: i32) -> DeltaResult {
        let old_current = self.current;
        let raw = self.current.saturating_add(delta);
        self.current = raw.clamp(0, self.max);
        let crossed = detect_crossings(&self.thresholds, old_current, self.current);
        DeltaResult {
            new_current: self.current,
            crossed,
        }
    }
}

impl Combatant for CreatureCore {
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn edge(&self) -> i32 {
        self.edge.current
    }
    fn max_edge(&self) -> i32 {
        self.edge.max
    }
    fn level(&self) -> u32 {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_core() -> CreatureCore {
        CreatureCore {
            name: NonBlankString::new("Test Creature").unwrap(),
            description: NonBlankString::new("A test creature").unwrap(),
            personality: NonBlankString::new("Testy").unwrap(),
            level: 3,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
            edge: placeholder_edge_pool(),
            acquired_advancements: vec![],
        }
    }

    #[test]
    fn combatant_accessors() {
        let c = test_core();
        assert_eq!(c.name(), "Test Creature");
        assert_eq!(Combatant::edge(&c), PLACEHOLDER_EDGE_BASE_MAX);
        assert_eq!(Combatant::max_edge(&c), PLACEHOLDER_EDGE_BASE_MAX);
        assert_eq!(Combatant::level(&c), 3);
    }

    #[test]
    fn combatant_not_broken_at_full_edge() {
        let c = test_core();
        assert!(!c.is_broken());
    }

    #[test]
    fn combatant_broken_when_edge_drained() {
        let mut c = test_core();
        c.edge.current = 0;
        assert!(c.is_broken());
    }

    #[test]
    fn apply_delta_debits_edge() {
        let mut c = test_core();
        let result = c.edge.apply_delta(-3);
        assert_eq!(result.new_current, PLACEHOLDER_EDGE_BASE_MAX - 3);
        assert_eq!(c.edge.current, PLACEHOLDER_EDGE_BASE_MAX - 3);
    }

    #[test]
    fn apply_delta_floors_at_zero() {
        let mut c = test_core();
        c.edge.apply_delta(-1000);
        assert_eq!(c.edge.current, 0);
    }

    #[test]
    fn apply_delta_caps_at_max() {
        let mut c = test_core();
        c.edge.current = 5;
        c.edge.apply_delta(1000);
        assert_eq!(c.edge.current, c.edge.max);
    }

    #[test]
    fn json_roundtrip() {
        let c = test_core();
        let json = serde_json::to_string(&c).unwrap();
        let back: CreatureCore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.as_str(), "Test Creature");
        assert_eq!(back.edge.base_max, PLACEHOLDER_EDGE_BASE_MAX);
        assert_eq!(back.level, 3);
        assert!(back.acquired_advancements.is_empty());
    }

    #[test]
    fn placeholder_edge_pool_has_on_resolution_trigger() {
        let pool = placeholder_edge_pool();
        assert!(pool.recovery_triggers.contains(&RecoveryTrigger::OnResolution));
        assert_eq!(pool.base_max, PLACEHOLDER_EDGE_BASE_MAX);
    }
}
