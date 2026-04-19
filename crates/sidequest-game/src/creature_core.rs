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
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::combatant::Combatant;
use crate::inventory::Inventory;
use crate::thresholds::{detect_crossings, ThresholdAt};
use sidequest_genre::EdgeConfig;

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

/// Error returned when an `EdgeConfig` does not declare `base_max_by_class`
/// for the requested class.
///
/// Intentionally loud — matches the "no silent fallbacks" project rule. A
/// heavy_metal character whose class is missing from the config fails chargen
/// rather than silently reverting to the placeholder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("edge_config.base_max_by_class missing entry for class '{0}'")]
pub struct EdgeConfigMissingClassError(String);

impl EdgeConfigMissingClassError {
    /// Construct an error for the given missing class.
    pub fn new(class: impl Into<String>) -> Self {
        Self(class.into())
    }
    /// The class name that was missing from `base_max_by_class`.
    pub fn class(&self) -> &str {
        &self.0
    }
    /// Consume the error and return the class name (used by callers that
    /// rewrap the error into a domain-specific variant).
    pub fn into_class(self) -> String {
        self.0
    }
}

/// Build a genre-authored `EdgePool` from an `EdgeConfig` and a class name.
///
/// Resolves `base_max` from `edge_config.base_max_by_class[class]` (fails
/// loudly when absent), converts every `EdgeThresholdDecl` to an
/// `EdgeThreshold`, and seeds `recovery_triggers` with `OnResolution`
/// (39-4/5/6 will layer additional triggers via `AdvancementEffect`).
/// The crossing-direction tag from YAML is informational — all EdgePool
/// thresholds fire on downward crossings by construction.
pub fn edge_pool_from_config(
    edge_config: &EdgeConfig,
    class: &str,
) -> Result<EdgePool, EdgeConfigMissingClassError> {
    let base_max = edge_config
        .base_max_by_class
        .get(class)
        .copied()
        .ok_or_else(|| EdgeConfigMissingClassError::new(class))?;
    let thresholds = edge_config
        .thresholds
        .iter()
        .map(|decl| EdgeThreshold {
            at: decl.at,
            event_id: decl.event_id.clone(),
            narrator_hint: decl.narrator_hint.clone(),
        })
        .collect();
    Ok(EdgePool {
        current: base_max,
        max: base_max,
        base_max,
        recovery_triggers: vec![RecoveryTrigger::OnResolution],
        thresholds,
    })
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

// `RecoveryTrigger` moved to `sidequest-genre::models::advancement` in
// Story 39-5 so `AdvancementEffect::EdgeRecovery { trigger: RecoveryTrigger }`
// can live in the genre crate without a game→genre→game cycle. This
// re-export keeps the historical path working.
pub use sidequest_genre::RecoveryTrigger;

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

impl CreatureCore {
    /// Apply an edge delta to this creature's composure pool and emit
    /// the canonical `creature.hp_delta` WatcherEvent + tracing span.
    ///
    /// Story 39-2: replaces the deleted `apply_hp_delta`. Story 39-4 wires
    /// dispatch-level `edge_delta` routing; this method is the per-creature
    /// mutation point that every caller should route through so the GM
    /// panel observes the composure change. The event name is retained as
    /// `creature.hp_delta` for dashboard continuity — 39-7 renames it
    /// alongside the wire rename.
    pub fn apply_edge_delta(&mut self, delta: i32) -> DeltaResult {
        let old_current = self.edge.current;
        let result = self.edge.apply_delta(delta);
        let new_current = self.edge.current;
        let clamped = new_current != old_current.saturating_add(delta);

        WatcherEventBuilder::new("creature", WatcherEventType::StateTransition)
            .field("action", "hp_delta")
            .field("name", self.name.as_str())
            .field("old_hp", old_current)
            .field("new_hp", new_current)
            .field("delta", delta)
            .field("max_hp", self.edge.max)
            .field("clamped", clamped)
            .send();

        let span = tracing::info_span!(
            "creature.hp_delta",
            name = %self.name,
            old_hp = old_current,
            new_hp = new_current,
            delta = delta,
            clamped = clamped,
        );
        let _guard = span.enter();

        result
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
        assert!(pool
            .recovery_triggers
            .contains(&RecoveryTrigger::OnResolution));
        assert_eq!(pool.base_max, PLACEHOLDER_EDGE_BASE_MAX);
    }
}
