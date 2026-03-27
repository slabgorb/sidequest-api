//! Scene directive formatter — composing fired beats, narrative hints,
//! and active stakes into a MUST-weave narrator block.
//!
//! Story 6-1: Pure function that collects inputs from the trope engine
//! and world state, then builds a `SceneDirective` for prompt injection.

use crate::trope::FiredBeat;
use serde::{Deserialize, Serialize};

/// Default maximum number of mandatory elements in a scene directive.
const DEFAULT_MAX_ELEMENTS: usize = 3;

/// Priority level for a directive element, derived from beat urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DirectivePriority {
    /// Low priority — minor flavor elements.
    Low,
    /// Medium priority — notable events and active stakes.
    Medium,
    /// High priority — critical escalation beats.
    High,
}

impl DirectivePriority {
    /// Map a beat urgency threshold (0.0–1.0) to a priority level.
    pub fn from_beat_urgency(urgency: f64) -> Self {
        if urgency >= 0.7 {
            Self::High
        } else if urgency >= 0.3 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// Source of a directive element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DirectiveSource {
    /// From a fired trope escalation beat.
    TropeBeat,
    /// From an active stake in the game state.
    ActiveStake,
}

/// A single element within a scene directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectiveElement {
    /// Where this element originated.
    pub source: DirectiveSource,
    /// The narrative content to weave into the response.
    pub content: String,
    /// How important this element is.
    pub priority: DirectivePriority,
}

/// An active stake in the game world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveStake {
    /// Description of what is at stake.
    pub description: String,
}

/// A composed scene directive for the narrator prompt.
pub struct SceneDirective {
    /// Elements the narrator MUST weave into the response.
    pub mandatory_elements: Vec<DirectiveElement>,
    /// Faction-driven events (wired in story 6-5).
    pub faction_events: Vec<String>,
    /// Narrative hints passed through as-is.
    pub narrative_hints: Vec<String>,
}

/// Compose fired beats, active stakes, and narrative hints into a scene directive.
///
/// Elements are sorted by priority descending and capped at `DEFAULT_MAX_ELEMENTS`.
pub fn format_scene_directive(
    fired_beats: &[FiredBeat],
    active_stakes: &[ActiveStake],
    narrative_hints: &[String],
) -> SceneDirective {
    let mut elements = Vec::new();

    for beat in fired_beats {
        elements.push(DirectiveElement {
            source: DirectiveSource::TropeBeat,
            content: beat.beat.event.clone(),
            priority: DirectivePriority::from_beat_urgency(beat.beat.at),
        });
    }

    for stake in active_stakes {
        elements.push(DirectiveElement {
            source: DirectiveSource::ActiveStake,
            content: stake.description.clone(),
            priority: DirectivePriority::Medium,
        });
    }

    elements.sort_by(|a, b| b.priority.cmp(&a.priority));
    elements.truncate(DEFAULT_MAX_ELEMENTS);

    SceneDirective {
        mandatory_elements: elements,
        faction_events: vec![],
        narrative_hints: narrative_hints.to_vec(),
    }
}
