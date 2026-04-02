//! Post-narration assembly and mechanical preprocessors (ADR-057).
//!
//! This module separates "crunch" (mechanical text analysis) from "fluff"
//! (LLM narration). Preprocessors run before the narrator call and produce
//! `ActionFlags` and `ActionRewrite` without any LLM involvement. Tool calls
//! (`set_mood`, `set_intent`, `scene_render`) validate typed enum values.
//! The `assemble_turn` function merges narrator output with preprocessor and
//! tool call results, with preprocessor/tool values always taking precedence.

pub mod assemble_turn;
pub mod personality_event;
pub mod play_sfx;
pub mod preprocessors;
pub mod quest_update;
pub mod resource_change;
pub mod scene_render;
pub mod set_intent;
pub mod set_mood;
pub mod tool_call_parser;
