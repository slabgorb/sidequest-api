//! OCEAN personality profile — Big Five traits for NPCs (Story 10-1).

use serde::{Deserialize, Deserializer, Serialize};

/// Clamp a value to the 0.0–10.0 range.
fn clamp_dimension(v: f64) -> f64 {
    v.clamp(0.0, 10.0)
}

/// Deserialize an f64 and clamp it to [0.0, 10.0].
fn deserialize_clamped<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(deserializer)?;
    Ok(clamp_dimension(v))
}

/// Big Five (OCEAN) personality profile.
///
/// Each dimension is an f64 in the range 0.0–10.0. Out-of-range values are
/// clamped on deserialization. Default is 5.0 (neutral) for all dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OceanProfile {
    /// Openness to experience.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub openness: f64,
    /// Conscientiousness.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub conscientiousness: f64,
    /// Extraversion.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub extraversion: f64,
    /// Agreeableness.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub agreeableness: f64,
    /// Neuroticism.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub neuroticism: f64,
}

fn neutral() -> f64 {
    5.0
}

impl Default for OceanProfile {
    fn default() -> Self {
        Self {
            openness: 5.0,
            conscientiousness: 5.0,
            extraversion: 5.0,
            agreeableness: 5.0,
            neuroticism: 5.0,
        }
    }
}
