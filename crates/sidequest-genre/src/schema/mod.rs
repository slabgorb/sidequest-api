//! Per-tier content schemas. Each tier is a distinct type. Serde's
//! `deny_unknown_fields` plus the schema split makes cross-tier content
//! leaks (e.g., a funnel at genre level) a load-time failure.

/// Culture-tier content schema: terminal flavor pass (names, speech, visual cues).
pub mod culture;
/// Genre-tier content schema: structural patterns and constraints, no named instances.
pub mod genre;
/// Global-tier content schema: genre-agnostic structural primitives.
pub mod global;
/// World-tier content schema: named instances (funnels, factions, leitmotifs).
pub mod world;
