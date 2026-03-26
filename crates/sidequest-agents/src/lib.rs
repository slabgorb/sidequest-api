//! SideQuest Agents — Claude CLI subprocess orchestration.
//!
//! This crate manages Claude CLI subprocess invocations for agent tasks,
//! including prompt composition and response parsing.

#![warn(missing_docs)]

pub mod agent;
pub mod agents;
pub mod client;
pub mod context_builder;
pub mod entity_reference;
pub mod exercise_tracker;
pub mod extractor;
pub mod format_helpers;
pub mod orchestrator;
pub mod patch_legality;
pub mod patches;
pub mod prompt_framework;
pub mod trope_alignment;
pub mod turn_record;

pub use sidequest_game;
pub use sidequest_protocol;
