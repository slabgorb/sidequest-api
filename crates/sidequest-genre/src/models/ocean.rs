//! Pacing thresholds and OCEAN personality profile types.

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};

// ═══════════════════════════════════════════════════════════
// Pacing thresholds (loaded from pacing.yaml, consumed by game crate)
// ═══════════════════════════════════════════════════════════

/// Genre-tunable breakpoints for pacing decisions.
///
/// Loaded from an optional `pacing.yaml` in the genre pack directory.
/// Missing fields fall back to defaults via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DramaThresholds {
    /// Drama weight at or above which delivery switches from Instant to Sentence.
    pub sentence_delivery_min: f64,
    /// Drama weight above which delivery switches from Sentence to Streaming.
    pub streaming_delivery_min: f64,
    /// Drama weight above which image rendering is triggered (beat filter).
    pub render_threshold: f64,
    /// Consecutive boring turns before an escalation beat hint is injected.
    pub escalation_streak: u32,
    /// Number of boring turns to reach action_tension 1.0 (gambler's ramp length).
    pub ramp_length: u32,
}

impl Default for DramaThresholds {
    fn default() -> Self {
        Self {
            sentence_delivery_min: 0.30,
            streaming_delivery_min: 0.70,
            render_threshold: 0.40,
            escalation_streak: 5,
            ramp_length: 8,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// OCEAN personality profile (loaded from archetypes.yaml, consumed by game crate)
// ═══════════════════════════════════════════════════════════

/// Clamp a value to the 0.0–10.0 range.
fn clamp_dimension(v: f64) -> f64 {
    v.clamp(0.0, 10.0)
}

/// Deserialize an f64 and clamp it to [0.0, 10.0].
fn deserialize_clamped<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(deserializer)?;
    Ok(clamp_dimension(v))
}

fn neutral() -> f64 {
    5.0
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

impl OceanProfile {
    /// Generate a fully random OCEAN profile with values in [0.0, 10.0].
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self {
            openness: rng.random_range(0.0..=10.0),
            conscientiousness: rng.random_range(0.0..=10.0),
            extraversion: rng.random_range(0.0..=10.0),
            agreeableness: rng.random_range(0.0..=10.0),
            neuroticism: rng.random_range(0.0..=10.0),
        }
    }

    /// Produce a natural-language behavioral summary from OCEAN scores.
    ///
    /// Dimensions with extreme scores (low 0–3, high 7–10) contribute
    /// adjectives; mid-range dimensions are omitted. An all-neutral profile
    /// returns a fallback phrase.
    pub fn behavioral_summary(&self) -> String {
        let dimensions: &[(f64, &str, &str)] = &[
            (self.openness, "conventional and practical", "curious and imaginative"),
            (self.conscientiousness, "spontaneous and flexible", "meticulous and disciplined"),
            (self.extraversion, "reserved and quiet", "outgoing and talkative"),
            (self.agreeableness, "competitive and blunt", "cooperative and empathetic"),
            (self.neuroticism, "calm and steady", "anxious and volatile"),
        ];

        let descriptors: Vec<&str> = dimensions
            .iter()
            .filter_map(|&(score, low, high)| {
                if score <= 3.0 {
                    Some(low)
                } else if score >= 7.0 {
                    Some(high)
                } else {
                    None
                }
            })
            .collect();

        match descriptors.len() {
            0 => "balanced temperament".to_string(),
            1 => descriptors[0].to_string(),
            _ => {
                let (last, rest) = descriptors.split_last().unwrap();
                format!("{}, and {last}", rest.join(", "))
            }
        }
    }

    /// Return a new profile jittered by up to ±`max_delta` per dimension,
    /// clamped to [0.0, 10.0].
    pub fn with_jitter(&self, max_delta: f64) -> Self {
        let mut rng = rand::rng();
        let mut jitter = |base: f64| -> f64 {
            let offset = rng.random_range(-max_delta..=max_delta);
            clamp_dimension(base + offset)
        };
        Self {
            openness: jitter(self.openness),
            conscientiousness: jitter(self.conscientiousness),
            extraversion: jitter(self.extraversion),
            agreeableness: jitter(self.agreeableness),
            neuroticism: jitter(self.neuroticism),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// OCEAN dimension enum & shift log (story 10-5)
// ═══════════════════════════════════════════════════════════

/// One of the Big Five personality dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OceanDimension {
    /// Openness to experience.
    Openness,
    /// Conscientiousness.
    Conscientiousness,
    /// Extraversion.
    Extraversion,
    /// Agreeableness.
    Agreeableness,
    /// Neuroticism.
    Neuroticism,
}

/// A single recorded personality shift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OceanShift {
    /// Which OCEAN dimension changed.
    pub dimension: OceanDimension,
    /// Value before the shift.
    pub old_value: f64,
    /// Value after the shift (clamped to 0.0–10.0).
    pub new_value: f64,
    /// Free-text reason for the change.
    pub cause: String,
    /// Game turn when the shift occurred.
    pub turn: u32,
}

/// Append-only log of personality shifts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OceanShiftLog {
    shifts: Vec<OceanShift>,
}

impl OceanShiftLog {
    /// Append a shift entry.
    pub fn push(&mut self, shift: OceanShift) {
        self.shifts.push(shift);
    }

    /// Return all recorded shifts.
    pub fn shifts(&self) -> &[OceanShift] {
        &self.shifts
    }

    /// Return shifts for a specific dimension.
    pub fn shifts_for(&self, dimension: OceanDimension) -> Vec<&OceanShift> {
        self.shifts.iter().filter(|s| s.dimension == dimension).collect()
    }
}

impl OceanProfile {
    /// Apply a delta to a single dimension, clamp, log the shift, and return
    /// the new value.
    pub fn apply_shift(
        &mut self,
        dimension: OceanDimension,
        delta: f64,
        cause: String,
        turn: u32,
        log: &mut OceanShiftLog,
    ) -> f64 {
        let old_value = self.get(dimension);
        let new_value = (old_value + delta).clamp(0.0, 10.0);
        match dimension {
            OceanDimension::Openness => self.openness = new_value,
            OceanDimension::Conscientiousness => self.conscientiousness = new_value,
            OceanDimension::Extraversion => self.extraversion = new_value,
            OceanDimension::Agreeableness => self.agreeableness = new_value,
            OceanDimension::Neuroticism => self.neuroticism = new_value,
        }
        log.push(OceanShift {
            dimension,
            old_value,
            new_value,
            cause,
            turn,
        });
        new_value
    }

    /// Read a dimension's current value.
    pub fn get(&self, dimension: OceanDimension) -> f64 {
        match dimension {
            OceanDimension::Openness => self.openness,
            OceanDimension::Conscientiousness => self.conscientiousness,
            OceanDimension::Extraversion => self.extraversion,
            OceanDimension::Agreeableness => self.agreeableness,
            OceanDimension::Neuroticism => self.neuroticism,
        }
    }
}
