//! SideQuest Agents — Claude CLI subprocess orchestration.
//!
//! This crate manages Claude CLI subprocess invocations for agent tasks,
//! including prompt composition and response parsing.

#![warn(missing_docs)]

pub mod agent;
pub mod agents;
pub mod client;
pub mod context_builder;
pub mod continuity_validator;
pub mod entity_reference;
pub mod exercise_tracker;
pub mod footnotes;
pub mod inventory_extractor;
// format_helpers module removed — superseded by inline formatting in
// sidequest-server::dispatch::prompt::build_prompt_context.
pub mod lore_filter;
pub mod orchestrator;
pub mod patch_legality;
pub mod patches;
pub mod preprocessor;
pub mod prompt_framework;
pub mod tools;
pub mod turn_record;

pub use sidequest_game;
pub use sidequest_protocol;
