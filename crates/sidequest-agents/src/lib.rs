//! SideQuest Agents — Claude CLI subprocess orchestration.
//!
//! This crate manages Claude CLI subprocess invocations for agent tasks,
//! including prompt composition and response parsing.

#![warn(missing_docs)]

pub mod agent;
pub mod client;
pub mod context_builder;
pub mod extractor;
pub mod format_helpers;
pub mod prompt_framework;

pub use sidequest_game;
pub use sidequest_protocol;
