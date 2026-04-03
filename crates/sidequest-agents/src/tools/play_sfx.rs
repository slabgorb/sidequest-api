//! Play SFX validation tool (ADR-057 Phase 7).
//!
//! Validates an SFX ID against the genre's loaded SFX library. The LLM decides
//! THAT a sound effect should play; this tool validates the ID exists.

/// A validated play SFX result from a tool call.
///
/// Produced by `validate_play_sfx`. Fields are private with getters
/// to prevent post-construction mutation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaySfxResult {
    sfx_id: String,
}

impl PlaySfxResult {
    /// The validated SFX ID (lowercased, trimmed).
    pub fn sfx_id(&self) -> &str {
        &self.sfx_id
    }
}

/// Error returned when SFX ID is invalid.
#[derive(Debug, thiserror::Error)]
#[error("invalid SFX ID: {0}")]
pub struct InvalidSfxId(String);

/// Validate an SFX ID against the loaded library (case-insensitive).
///
/// - `sfx_id`: the SFX identifier to validate
/// - `library`: available SFX IDs from the genre's audio config
#[tracing::instrument(name = "tool.play_sfx", skip_all, fields(sfx_id = %sfx_id))]
pub fn validate_play_sfx(
    sfx_id: &str,
    library: &[String],
) -> Result<PlaySfxResult, InvalidSfxId> {
    let trimmed = sfx_id.trim();
    if trimmed.is_empty() {
        tracing::warn!(valid = false, "play_sfx validation failed — empty SFX ID");
        return Err(InvalidSfxId("SFX ID must not be empty".to_string()));
    }

    let lowered = trimmed.to_lowercase();
    let matched = library
        .iter()
        .find(|id| id.to_lowercase() == lowered);

    match matched {
        Some(_) => {
            let result = PlaySfxResult {
                sfx_id: lowered,
            };

            tracing::info!(
                valid = true,
                sfx_id = result.sfx_id.as_str(),
                "play_sfx validated"
            );

            Ok(result)
        }
        None => {
            tracing::warn!(valid = false, attempted_id = %sfx_id, "play_sfx validation failed — unknown SFX ID");
            Err(InvalidSfxId(format!(
                "unknown SFX ID \"{trimmed}\" — not in loaded library"
            )))
        }
    }
}
