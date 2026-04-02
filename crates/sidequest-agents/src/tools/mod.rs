//! Post-narration assembly and mechanical preprocessors (ADR-057 Phase 1).
//!
//! This module separates "crunch" (mechanical text analysis) from "fluff"
//! (LLM narration). Preprocessors run before the narrator call and produce
//! `ActionFlags` and `ActionRewrite` without any LLM involvement. The
//! `assemble_turn` function merges narrator output with preprocessor results,
//! with preprocessor values always taking precedence.

pub mod assemble_turn;
pub mod preprocessors;
