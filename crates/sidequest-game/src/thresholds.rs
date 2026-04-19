//! Shared threshold-crossing helpers (story 39-1).
//!
//! Extracted from `resource_pool.rs` so `ResourcePool` (f64-valued) and
//! `EdgePool` (i32-valued composure currency, epic 39) can mint the same
//! kind of LoreFragment-via-event when a pool value crosses a named
//! threshold downward. Before 39-1 these helpers lived privately in
//! `resource_pool.rs`; making them generic and module-scoped is the
//! refactor that pays down duplication before the new EdgePool starts
//! copying it.
//!
//! # Semantics
//!
//! - `detect_crossings` returns thresholds where `old > at && new <= at`.
//!   Upward transitions never fire. Landing on `at` from above fires;
//!   already being at `at` and holding does not.
//! - `mint_threshold_lore` turns each crossed threshold into a
//!   `LoreFragment` in the `Event` category — high-relevance for
//!   narrator context selection — keyed by the threshold's event_id.
//!   Duplicate ids are silently skipped (LoreStore rejects them).

use std::collections::HashMap;

use crate::lore::{LoreCategory, LoreFragment, LoreSource, LoreStore};

/// A threshold that can be tested against a pool value.
///
/// Implemented by `ResourceThreshold` (Value = f64) and `EdgeThreshold`
/// (Value = i32) so both pool types route their crossing detection and
/// lore minting through the same helpers.
pub trait ThresholdAt {
    /// The numeric type of the pool this threshold applies to.
    type Value: PartialOrd + Copy;

    /// The value at which this threshold fires (crossed downward).
    fn at(&self) -> Self::Value;
    /// Event identifier emitted when this threshold is crossed.
    fn event_id(&self) -> &str;
    /// Narrator hint injected when this threshold is crossed.
    fn narrator_hint(&self) -> &str;
}

/// Detect which thresholds were crossed by a value change (downward only).
///
/// A threshold `t` is crossed when `old > t.at() && new <= t.at()`.
pub fn detect_crossings<T>(thresholds: &[T], old_value: T::Value, new_value: T::Value) -> Vec<T>
where
    T: ThresholdAt + Clone,
{
    thresholds
        .iter()
        .filter(|t| old_value > t.at() && new_value <= t.at())
        .cloned()
        .collect()
}

/// Mint `LoreFragment`s from threshold crossings.
///
/// Each crossed threshold becomes a `LoreFragment` with:
/// - id: the threshold's `event_id`
/// - category: `Event` (high relevance for narrator context selection)
/// - content: the threshold's `narrator_hint`
/// - source: `GameEvent`
/// - turn_created: the supplied turn number
///
/// Duplicate ids are silently skipped (LoreStore rejects them).
pub fn mint_threshold_lore<T: ThresholdAt>(thresholds: &[T], store: &mut LoreStore, turn: u64) {
    for threshold in thresholds {
        let fragment = LoreFragment::new(
            threshold.event_id().to_string(),
            LoreCategory::Event,
            threshold.narrator_hint().to_string(),
            LoreSource::GameEvent,
            Some(turn),
            HashMap::new(),
        );
        let _ = store.add(fragment);
    }
}
