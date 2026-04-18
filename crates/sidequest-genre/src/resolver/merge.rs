//! Per-field merge strategies used by `Resolver<T>` when walking
//! Global -> Genre -> World -> Culture.

use serde::{Deserialize, Serialize};

/// Per-field merge strategy. Annotated on each `Layered` struct field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Deeper tier's value wins outright when present.
    Replace,
    /// Deeper tier's list concatenates onto base's. Helper: `apply_append`.
    Append,
    /// Struct-walked merge — `Layered` derive implements this per field.
    DeepMerge,
    /// Only the Culture tier may set this field. Genre/World cannot.
    CultureFinal,
}

/// Apply `Replace`/`CultureFinal`-style semantics: deeper tier wins when
/// present, otherwise fall back to base. For the scalar case. `Append`
/// and `DeepMerge` operate at the struct-walk level in the derive macro.
pub fn apply_strategy<T: Clone>(
    _strategy: MergeStrategy,
    base: Option<T>,
    deeper: Option<T>,
) -> Option<T> {
    deeper.or(base)
}

/// Append-strategy helper: deeper tier's list concatenates onto base's.
pub fn apply_append<T: Clone>(base: &[T], deeper: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(base.len() + deeper.len());
    out.extend_from_slice(base);
    out.extend_from_slice(deeper);
    out
}
