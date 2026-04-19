//! CreatureCore — shared fields and behavior for Character and NPC.
//!
//! Story 1-13: Extracted from Character and NPC to eliminate duplication.
//! Both types embed `CreatureCore` via composition.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::combatant::Combatant;
use crate::hp::clamp_hp;
use crate::inventory::Inventory;
use crate::thresholds::{detect_crossings, ThresholdAt};

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
    /// Current hit points (0..=max_hp).
    pub hp: i32,
    /// Maximum hit points (>= 1).
    pub max_hp: i32,
    /// Armor class.
    pub ac: i32,
    /// Experience points accumulated.
    #[serde(default)]
    pub xp: u32,
    /// Inventory of carried items.
    pub inventory: Inventory,
    /// Active status conditions.
    pub statuses: Vec<String>,
}

impl CreatureCore {
    /// Apply HP damage or healing, clamped to [0, max_hp].
    ///
    /// Emits a `creature.hp_delta` watcher event so the GM panel can verify
    /// HP mutations. Story 28-2 added the WatcherEventBuilder emission;
    /// story 28-12 added the parallel `tracing::info_span!` for log-level
    /// visibility — both channels stay because they serve different
    /// audiences (broadcast for GM panel, tracing for stdout/jaeger).
    pub fn apply_hp_delta(&mut self, delta: i32) {
        let old_hp = self.hp;
        self.hp = clamp_hp(self.hp, delta, self.max_hp);
        let clamped = self.hp != old_hp + delta;

        // OTEL: creature.hp_delta — broadcast for the GM panel (story 28-2).
        WatcherEventBuilder::new("creature", WatcherEventType::StateTransition)
            .field("action", "hp_delta")
            .field("name", self.name.as_str())
            .field("old_hp", old_hp)
            .field("new_hp", self.hp)
            .field("delta", delta)
            .field("max_hp", self.max_hp)
            .field("clamped", clamped)
            .send();

        // tracing span — for log/jaeger consumers (story 28-12).
        let span = tracing::info_span!(
            "creature.hp_delta",
            name = %self.name,
            old_hp = old_hp,
            new_hp = self.hp,
            delta = delta,
            clamped = clamped,
        );
        let _guard = span.enter();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Epic 39 — EdgePool composure currency (story 39-1)
//
// EdgePool is the first-class composure pool that replaces phantom HP as
// the axis combat, social, and pressure scenes swing on. Story 39-1 lands
// the types only — no `edge` field on CreatureCore yet (that's 39-2), no
// dispatch wiring (that's 39-4). Helpers route through
// `crate::thresholds` so ResourcePool and EdgePool mint LoreFragments via
// the same code path.
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
///
/// Story 39-1 lands the type + helpers. It is intentionally *not* yet
/// referenced from `CreatureCore`, `dispatch/`, or `server/` — that
/// wiring lands in 39-2 and 39-4. AC5 enforces this: `EdgePool` must
/// appear only in `creature_core.rs` and its tests at this point.
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
    fn hp(&self) -> i32 {
        self.hp
    }
    fn max_hp(&self) -> i32 {
        self.max_hp
    }
    fn level(&self) -> u32 {
        self.level
    }
    fn ac(&self) -> i32 {
        self.ac
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
            hp: 20,
            max_hp: 30,
            ac: 15,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        }
    }

    #[test]
    fn combatant_accessors() {
        let c = test_core();
        assert_eq!(c.name(), "Test Creature");
        assert_eq!(Combatant::hp(&c), 20);
        assert_eq!(Combatant::max_hp(&c), 30);
        assert_eq!(Combatant::level(&c), 3);
        assert_eq!(Combatant::ac(&c), 15);
    }

    #[test]
    fn combatant_is_alive() {
        let c = test_core();
        assert!(c.is_alive());
    }

    #[test]
    fn apply_damage() {
        let mut c = test_core();
        c.apply_hp_delta(-10);
        assert_eq!(c.hp, 10);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut c = test_core();
        c.apply_hp_delta(100);
        assert_eq!(c.hp, 30);
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut c = test_core();
        c.apply_hp_delta(-100);
        assert_eq!(c.hp, 0);
    }

    #[test]
    fn json_roundtrip() {
        let c = test_core();
        let json = serde_json::to_string(&c).unwrap();
        let back: CreatureCore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.as_str(), "Test Creature");
        assert_eq!(back.hp, 20);
        assert_eq!(back.level, 3);
    }
}
