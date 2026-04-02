//! Resource change validation tool (ADR-057 Phase 7).
//!
//! Validates resource name against genre-declared resource names and structures
//! the delta. The LLM decides THAT a resource changed; this tool validates
//! the resource name exists and the delta is finite.

/// A validated resource change result from a tool call.
///
/// Produced by `validate_resource_change`. Fields are private with getters
/// to prevent post-construction mutation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceChangeResult {
    resource: String,
    delta: f64,
}

impl ResourceChangeResult {
    /// The resource name (lowercased, trimmed, validated against genre declarations).
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// The signed delta (positive = gained, negative = spent/lost).
    pub fn delta(&self) -> f64 {
        self.delta
    }
}

/// Error returned when resource change inputs are invalid.
#[derive(Debug, thiserror::Error)]
#[error("invalid resource change: {0}")]
pub struct InvalidResourceChange(String);

/// Validate a resource change from a tool call.
///
/// - `resource`: resource name (case-insensitive match against `valid_resources`)
/// - `delta`: signed numeric change (must be finite — no NaN or Infinity)
/// - `valid_resources`: genre-declared resource names to validate against
#[tracing::instrument(name = "tool.resource_change", skip_all, fields(resource = %resource, delta = %delta))]
pub fn validate_resource_change(
    resource: &str,
    delta: f64,
    valid_resources: &[String],
) -> Result<ResourceChangeResult, InvalidResourceChange> {
    let trimmed = resource.trim();
    if trimmed.is_empty() {
        tracing::warn!(valid = false, "resource change validation failed — empty resource name");
        return Err(InvalidResourceChange("resource name must not be empty".to_string()));
    }

    if !delta.is_finite() {
        tracing::warn!(valid = false, "resource change validation failed — non-finite delta");
        return Err(InvalidResourceChange(format!(
            "delta must be finite, got {delta}"
        )));
    }

    let lowered = trimmed.to_lowercase();
    let matched = valid_resources
        .iter()
        .find(|r| r.to_lowercase() == lowered);

    match matched {
        Some(canonical) => {
            let result = ResourceChangeResult {
                resource: canonical.to_lowercase(),
                delta,
            };

            tracing::info!(
                valid = true,
                resource = result.resource.as_str(),
                delta = delta,
                "resource change validated"
            );

            Ok(result)
        }
        None => {
            tracing::warn!(valid = false, attempted_resource = %resource, "resource change validation failed — unknown resource");
            Err(InvalidResourceChange(format!(
                "unknown resource \"{trimmed}\" — expected one of: {}",
                valid_resources.join(", ")
            )))
        }
    }
}
