//! Quest update validation tool (ADR-057 Phase 6).
//!
//! Validates quest name and status string inputs from the LLM's tool call.
//! The LLM decides THAT a quest changed; this tool structures the transition.
//! Quest names and statuses are free-form strings (no enum — quests are open-ended).

/// A validated quest state transition.
///
/// Produced by `validate_quest_update`. Serializes to JSON for the tool call response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct QuestUpdate {
    /// The quest name (e.g., "The Corrupted Grove").
    pub quest_name: String,
    /// The new status string (e.g., "active: Find the source (from: Elder Mirova)").
    pub status: String,
}

/// Error returned when quest update inputs are invalid.
#[derive(Debug, thiserror::Error)]
#[error("invalid quest update: {0}")]
pub struct InvalidQuestUpdate(String);

/// Validate quest name and status, returning a structured `QuestUpdate`.
///
/// Rejects empty or whitespace-only inputs. Trims leading/trailing whitespace
/// from both fields.
#[tracing::instrument(name = "tool.quest_update", skip_all, fields(quest_name = %quest_name, status = %status))]
pub fn validate_quest_update(quest_name: &str, status: &str) -> Result<QuestUpdate, InvalidQuestUpdate> {
    let trimmed_name = quest_name.trim();
    let trimmed_status = status.trim();

    if trimmed_name.is_empty() {
        tracing::warn!(valid = false, "quest update validation failed — empty quest name");
        return Err(InvalidQuestUpdate("quest name must not be empty".to_string()));
    }

    if trimmed_status.is_empty() {
        tracing::warn!(valid = false, "quest update validation failed — empty status");
        return Err(InvalidQuestUpdate("status must not be empty".to_string()));
    }

    let update = QuestUpdate {
        quest_name: trimmed_name.to_string(),
        status: trimmed_status.to_string(),
    };

    tracing::info!(
        valid = true,
        quest_name = trimmed_name,
        status = trimmed_status,
        "quest update validated"
    );

    Ok(update)
}
