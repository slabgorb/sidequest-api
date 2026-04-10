//! Personality event validation tool (ADR-057 Phase 7).
//!
//! Validates NPC name, event_type string, and description from the LLM's tool call.
//! The LLM decides THAT a personality-shaping event occurred; this tool validates
//! the event_type against the `PersonalityEvent` enum from sidequest-game and
//! structures the result.

/// A validated personality event result from a tool call.
///
/// Produced by `validate_personality_event`. Fields are private with getters
/// to prevent post-construction mutation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonalityEventResult {
    npc: String,
    event_type: String,
    description: String,
}

impl PersonalityEventResult {
    /// The NPC's canonical name (trimmed).
    pub fn npc(&self) -> &str {
        &self.npc
    }

    /// The event type as a snake_case string (e.g., "betrayal", "near_death").
    pub fn event_type_str(&self) -> &str {
        &self.event_type
    }

    /// Optional description for OTEL telemetry context.
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Error returned when personality event inputs are invalid.
#[derive(Debug, thiserror::Error)]
#[error("invalid personality event: {0}")]
pub struct InvalidPersonalityEvent(String);

/// Valid event types — must match sidequest_game::PersonalityEvent variants.
const VALID_EVENT_TYPES: &[&str] = &[
    "betrayal",
    "near_death",
    "victory",
    "defeat",
    "social_bonding",
];

/// Validate a personality event from a tool call.
///
/// - `npc`: NPC canonical name (trimmed, must not be empty)
/// - `event_type`: one of betrayal, near_death, victory, defeat, social_bonding (case-insensitive)
/// - `description`: optional free-text context for OTEL (empty allowed)
#[tracing::instrument(name = "tool.personality_event", skip_all, fields(npc = %npc, event_type = %event_type))]
pub fn validate_personality_event(
    npc: &str,
    event_type: &str,
    description: &str,
) -> Result<PersonalityEventResult, InvalidPersonalityEvent> {
    let trimmed_npc = npc.trim();
    if trimmed_npc.is_empty() {
        tracing::warn!(
            valid = false,
            "personality event validation failed — empty NPC name"
        );
        return Err(InvalidPersonalityEvent(
            "NPC name must not be empty".to_string(),
        ));
    }

    let lowered = event_type.trim().to_lowercase();
    if !VALID_EVENT_TYPES.contains(&lowered.as_str()) {
        tracing::warn!(valid = false, attempted_type = %event_type, "personality event validation failed — invalid event_type");
        return Err(InvalidPersonalityEvent(format!(
            "invalid event_type \"{event_type}\" — expected one of: {}",
            VALID_EVENT_TYPES.join(", ")
        )));
    }

    let result = PersonalityEventResult {
        npc: trimmed_npc.to_string(),
        event_type: lowered,
        description: description.to_string(),
    };

    tracing::info!(
        valid = true,
        npc = trimmed_npc,
        event_type = result.event_type.as_str(),
        "personality event validated"
    );

    Ok(result)
}
