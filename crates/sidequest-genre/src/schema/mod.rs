//! Per-tier content schemas. Each tier is a distinct type. Serde's
//! `deny_unknown_fields` plus the schema split makes cross-tier content
//! leaks (e.g., a funnel at genre level) a load-time failure.

pub mod culture;
pub mod genre;
pub mod global;
pub mod world;
