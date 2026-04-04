//! Concrete agent implementations for the SideQuest engine.
//!
//! ADR-067: Unified narrator agent. CreatureSmith, Dialectician, and Ensemble
//! have been absorbed into the narrator via conditional prompt sections.

pub mod intent_router;
pub mod narrator;
pub mod resonator;
pub mod troper;
pub mod world_builder;
