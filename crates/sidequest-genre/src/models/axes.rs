//! Narrative axis configuration from `axes.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Narrative axis configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxesConfig {
    /// Axis definitions.
    pub definitions: Vec<AxisDefinition>,
    /// Per-axis pole modifiers (axis_id → pole_name → prompt text).
    #[serde(default)]
    pub modifiers: HashMap<String, HashMap<String, String>>,
    /// Named axis presets.
    #[serde(default)]
    pub presets: Vec<AxisPreset>,
}

/// A single narrative axis definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxisDefinition {
    /// Axis identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Low and high pole labels.
    pub poles: Vec<String>,
    /// Default value (0.0–1.0).
    pub default: f64,
}

/// A preset combination of axis values.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxisPreset {
    /// Preset name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Axis values.
    pub values: HashMap<String, f64>,
}
