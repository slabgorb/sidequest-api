//! ResourcePool — generic named resource with thresholds (story 16-10).
//!
//! A ResourcePool tracks a numeric value within [min, max] bounds, with optional
//! decay_per_turn, voluntary spending control, and threshold-based event detection.

use serde::{Deserialize, Serialize};

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

/// A named resource pool with bounded numeric value and optional thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    /// Pool name (e.g., "luck", "heat").
    pub name: String,
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

/// Detect which thresholds were crossed by a value change (downward crossing only).
///
/// A threshold at `t.at` is crossed when `old_value > t.at` and `new_value <= t.at`.
fn detect_crossings(
    thresholds: &[ResourceThreshold],
    old_value: f64,
    new_value: f64,
) -> Vec<ResourceThreshold> {
    thresholds
        .iter()
        .filter(|t| old_value > t.at && new_value <= t.at)
        .cloned()
        .collect()
}

impl ResourcePool {
    /// Apply a raw value change (unclamped delta or set), clamp, and detect threshold crossings.
    fn apply_and_clamp(
        &mut self,
        op: &ResourcePatchOp,
        value: f64,
    ) -> ResourcePatchResult {
        let old_value = self.current;
        let raw = match op {
            ResourcePatchOp::Add => self.current + value,
            ResourcePatchOp::Subtract => self.current - value,
            ResourcePatchOp::Set => value,
        };
        self.current = raw.clamp(self.min, self.max);

        let crossed = detect_crossings(&self.thresholds, old_value, self.current);

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

            let crossed = detect_crossings(&pool.thresholds, old_value, pool.current);
            all_crossings.extend(crossed);
        }

        all_crossings
    }

    /// Initialize resource pools from genre pack declarations.
    pub fn init_resource_pools(&mut self, declarations: &[sidequest_genre::ResourceDeclaration]) {
        for decl in declarations {
            let pool = ResourcePool {
                name: decl.name.clone(),
                current: decl.starting,
                min: decl.min,
                max: decl.max,
                voluntary: decl.voluntary,
                decay_per_turn: decl.decay_per_turn,
                thresholds: vec![],
            };
            self.resources.insert(decl.name.clone(), pool);
        }
    }
}
