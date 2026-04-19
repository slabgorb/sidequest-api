//! ResourcePool — generic named resource with thresholds (story 16-10).
//!
//! A ResourcePool tracks a numeric value within [min, max] bounds, with optional
//! decay_per_turn, voluntary spending control, and threshold-based event detection.
//! Story 16-11: threshold crossings mint LoreFragments for permanent narrator memory.

use serde::{Deserialize, Serialize};

use crate::lore::LoreStore;
use crate::thresholds::{self, ThresholdAt};

/// A threshold that fires an event when the pool value crosses below it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceThreshold {
    /// The value at which this threshold fires (crossed downward).
    pub at: f64,
    /// Event identifier emitted when this threshold is crossed.
    pub event_id: String,
    /// Hint text injected into narrator prompt when crossed.
    pub narrator_hint: String,
}

impl ThresholdAt for ResourceThreshold {
    type Value = f64;

    fn at(&self) -> f64 {
        self.at
    }
    fn event_id(&self) -> &str {
        &self.event_id
    }
    fn narrator_hint(&self) -> &str {
        &self.narrator_hint
    }
}

/// A named resource pool with bounded numeric value and optional thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    /// Pool name / internal ID (e.g., "luck", "heat").
    pub name: String,
    /// Display label shown to players and narrator (e.g., "Luck", "Heat").
    /// Defaults to empty for backward compat with old saves;
    /// `init_resource_pools()` upsert populates it from genre pack.
    #[serde(default)]
    pub label: String,
    /// Current value.
    pub current: f64,
    /// Minimum allowed value.
    pub min: f64,
    /// Maximum allowed value.
    pub max: f64,
    /// Whether the player can voluntarily spend (subtract) this resource.
    pub voluntary: bool,
    /// Automatic change per turn (positive = regen, negative = decay).
    pub decay_per_turn: f64,
    /// Thresholds that fire events when the value crosses below them.
    #[serde(default)]
    pub thresholds: Vec<ResourceThreshold>,
}

/// Operation to apply to a resource pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourcePatchOp {
    /// Add to current value.
    Add,
    /// Subtract from current value.
    Subtract,
    /// Set current value directly.
    Set,
}

/// A patch that modifies a single resource pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePatch {
    /// Name of the resource pool to modify.
    pub resource_name: String,
    /// The operation to perform.
    pub operation: ResourcePatchOp,
    /// The operand value.
    pub value: f64,
}

/// Result of applying a resource patch, including threshold crossings.
#[derive(Debug, Clone)]
pub struct ResourcePatchResult {
    /// Value before the patch was applied.
    pub old_value: f64,
    /// Value after the patch was applied (post-clamp).
    pub new_value: f64,
    /// Thresholds that were crossed (old_value was above, new_value is at or below).
    pub crossed_thresholds: Vec<ResourceThreshold>,
}

/// Error type for resource patch operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ResourcePatchError {
    /// The named resource does not exist.
    #[error("unknown resource: {0}")]
    UnknownResource(String),
    /// Player attempted to subtract from a non-voluntary resource.
    #[error("resource '{0}' is not voluntary — player cannot spend it")]
    NotVoluntary(String),
}

/// Mint LoreFragments from threshold crossings (story 16-11).
///
/// Thin wrapper that forwards to `crate::thresholds::mint_threshold_lore`,
/// the shared helper both ResourcePool and EdgePool route through as of
/// story 39-1. Duplicates are silently skipped (LoreStore rejects them).
pub fn mint_threshold_lore(thresholds: &[ResourceThreshold], store: &mut LoreStore, turn: u64) {
    thresholds::mint_threshold_lore(thresholds, store, turn);
}

impl ResourcePool {
    /// Apply a raw value change (unclamped delta or set), clamp, and detect threshold crossings.
    fn apply_and_clamp(&mut self, op: &ResourcePatchOp, value: f64) -> ResourcePatchResult {
        let old_value = self.current;
        let raw = match op {
            ResourcePatchOp::Add => self.current + value,
            ResourcePatchOp::Subtract => self.current - value,
            ResourcePatchOp::Set => value,
        };
        self.current = raw.clamp(self.min, self.max);

        let crossed = thresholds::detect_crossings(&self.thresholds, old_value, self.current);

        ResourcePatchResult {
            old_value,
            new_value: self.current,
            crossed_thresholds: crossed,
        }
    }
}

use crate::state::GameSnapshot;

impl GameSnapshot {
    /// Apply a resource patch (engine-level — ignores voluntary flag).
    pub fn apply_resource_patch(
        &mut self,
        patch: &ResourcePatch,
    ) -> Result<ResourcePatchResult, ResourcePatchError> {
        let pool = self
            .resources
            .get_mut(&patch.resource_name)
            .ok_or_else(|| ResourcePatchError::UnknownResource(patch.resource_name.clone()))?;

        Ok(pool.apply_and_clamp(&patch.operation, patch.value))
    }

    /// Apply a resource patch as a player action — rejects subtract on non-voluntary resources.
    pub fn apply_resource_patch_player(
        &mut self,
        patch: &ResourcePatch,
    ) -> Result<ResourcePatchResult, ResourcePatchError> {
        // Check voluntary flag for subtract operations
        if matches!(patch.operation, ResourcePatchOp::Subtract) {
            let pool = self
                .resources
                .get(&patch.resource_name)
                .ok_or_else(|| ResourcePatchError::UnknownResource(patch.resource_name.clone()))?;
            if !pool.voluntary {
                return Err(ResourcePatchError::NotVoluntary(
                    patch.resource_name.clone(),
                ));
            }
        }

        self.apply_resource_patch(patch)
    }

    /// Apply decay_per_turn to all resource pools. Returns all thresholds crossed.
    pub fn apply_pool_decay(&mut self) -> Vec<ResourceThreshold> {
        let mut all_crossings = Vec::new();

        for pool in self.resources.values_mut() {
            if pool.decay_per_turn.abs() < f64::EPSILON {
                continue;
            }
            let old_value = pool.current;
            let raw = pool.current + pool.decay_per_turn;
            pool.current = raw.clamp(pool.min, pool.max);

            let crossed = thresholds::detect_crossings(&pool.thresholds, old_value, pool.current);
            all_crossings.extend(crossed);
        }

        all_crossings
    }

    /// Initialize or upsert resource pools from genre pack declarations.
    ///
    /// **Upsert semantics (critical for save migration):**
    /// - If a pool with this name already exists (e.g., from a loaded save),
    ///   update its declaration-derived fields (label, min, max, voluntary,
    ///   decay_per_turn, thresholds) but **preserve the existing `current` value**.
    /// - If no pool exists, create a new one with `current = decl.starting`.
    ///
    /// This is what makes old saves migrate correctly: the legacy `resource_state`
    /// deserializer creates minimal ResourcePool entries with the saved `current`,
    /// then `init_resource_pools()` is called on session load to populate the
    /// genre-pack metadata without clobbering the player's progress.
    pub fn init_resource_pools(&mut self, declarations: &[sidequest_genre::ResourceDeclaration]) {
        for decl in declarations {
            let thresholds: Vec<ResourceThreshold> = decl
                .thresholds
                .iter()
                .map(|t| ResourceThreshold {
                    at: t.at,
                    event_id: t.event_id.clone(),
                    narrator_hint: t.narrator_hint.clone(),
                })
                .collect();

            if let Some(existing) = self.resources.get_mut(&decl.name) {
                // Preserve `current` — update everything else from genre pack.
                existing.label = decl.label.clone();
                existing.min = decl.min;
                existing.max = decl.max;
                existing.voluntary = decl.voluntary;
                existing.decay_per_turn = decl.decay_per_turn;
                existing.thresholds = thresholds;
                // Re-clamp in case the new bounds invalidate the saved value.
                existing.current = existing.current.clamp(existing.min, existing.max);
            } else {
                self.resources.insert(
                    decl.name.clone(),
                    ResourcePool {
                        name: decl.name.clone(),
                        label: decl.label.clone(),
                        current: decl.starting,
                        min: decl.min,
                        max: decl.max,
                        voluntary: decl.voluntary,
                        decay_per_turn: decl.decay_per_turn,
                        thresholds,
                    },
                );
            }
        }
    }

    /// Convenience method: apply a resource patch by name, op, and value.
    pub fn apply_resource_patch_by_name(
        &mut self,
        name: &str,
        op: ResourcePatchOp,
        value: f64,
    ) -> Result<ResourcePatchResult, ResourcePatchError> {
        let patch = ResourcePatch {
            resource_name: name.to_string(),
            operation: op,
            value,
        };
        self.apply_resource_patch(&patch)
    }

    /// Apply a resource patch and mint LoreFragments for any threshold crossings (story 16-11).
    pub fn process_resource_patch_with_lore(
        &mut self,
        name: &str,
        op: ResourcePatchOp,
        value: f64,
        store: &mut LoreStore,
        turn: u64,
    ) -> Result<ResourcePatchResult, ResourcePatchError> {
        let result = self.apply_resource_patch_by_name(name, op, value)?;
        mint_threshold_lore(&result.crossed_thresholds, store, turn);
        Ok(result)
    }
}
