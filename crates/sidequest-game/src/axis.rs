//! Narrative axis runtime — `/tone` command and prompt injection.
//!
//! Axis *definitions* live in `sidequest_genre::AxesConfig` (loaded from axes.yaml).
//! This module handles the runtime: current axis values persisted in GameSnapshot,
//! the `/tone` slash command, and `format_tone_context()` for narrator prompt injection.


use serde::{Deserialize, Serialize};

use crate::slash_router::{CommandHandler, CommandResult};
use crate::state::GameSnapshot;
use sidequest_genre::{AxesConfig, AxisDefinition};

/// A single axis value at runtime. Stored in GameSnapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisValue {
    /// Which axis this value belongs to (matches `AxisDefinition.id`).
    pub axis_id: String,
    /// Current value, clamped to [0.0, 1.0].
    pub value: f64,
}

/// `/tone` — View and manipulate narrative axis values.
///
/// Subcommands:
/// - `/tone` or `/tone show` — display current values with poles and presets
/// - `/tone preset {name}` — apply a named preset atomically
/// - `/tone set {axis_id} {value}` — set a single axis value
pub struct ToneCommand {
    /// Genre pack axis configuration (definitions, modifiers, presets).
    config: AxesConfig,
}

impl ToneCommand {
    /// Create a new ToneCommand with the given axis config from the genre pack.
    pub fn new(config: AxesConfig) -> Self {
        Self { config }
    }
}

impl CommandHandler for ToneCommand {
    fn name(&self) -> &str {
        "tone"
    }

    fn description(&self) -> &str {
        "View or adjust narrative tone axes"
    }

    fn handle(&self, state: &GameSnapshot, args: &str) -> CommandResult {
        let args = args.trim();
        if args.is_empty() || args.eq_ignore_ascii_case("show") {
            return self.handle_show(state);
        }

        let (sub, sub_args) = match args.split_once(' ') {
            Some((s, a)) => (s, a.trim()),
            None => (args, ""),
        };

        match sub.to_ascii_lowercase().as_str() {
            "show" => self.handle_show(state),
            "preset" => self.handle_preset(sub_args),
            "set" => self.handle_set(state, sub_args),
            other => CommandResult::Error(format!(
                "Unknown tone subcommand: '{}'. Usage: /tone [show|preset <name>|set <axis> <value>]",
                other
            )),
        }
    }
}

impl ToneCommand {
    fn handle_show(&self, state: &GameSnapshot) -> CommandResult {
        if self.config.definitions.is_empty() {
            return CommandResult::Display("No narrative axes defined for this genre.".to_string());
        }

        let mut output = String::from("NARRATIVE AXES:\n");

        for def in &self.config.definitions {
            let value = resolve_axis_value(&state.axis_values, def);
            let bar = render_bar(value);
            output.push_str(&format!(
                "  {} [{}] {} [{}]  ({:.2})\n",
                def.poles[0], bar_left(value), bar, bar_right(value), value,
            ));
            output.push_str(&format!("    {} — {}\n", def.name, def.description));
        }

        if !self.config.presets.is_empty() {
            output.push_str("\nPRESETS:\n");
            for preset in &self.config.presets {
                output.push_str(&format!("  {} — {}\n", preset.name, preset.description));
            }
        }

        CommandResult::Display(output)
    }

    fn handle_preset(&self, args: &str) -> CommandResult {
        if args.is_empty() {
            return CommandResult::Error(
                "Usage: /tone preset <name>".to_string(),
            );
        }

        let preset = self.config.presets.iter().find(|p| {
            p.name.eq_ignore_ascii_case(args)
        });

        let Some(preset) = preset else {
            let names: Vec<&str> = self.config.presets.iter().map(|p| p.name.as_str()).collect();
            return CommandResult::Error(format!(
                "Unknown preset '{}'. Available: {}",
                args,
                names.join(", ")
            ));
        };

        // Build axis values from preset, using defaults for missing axes.
        let mut values: Vec<AxisValue> = Vec::new();
        for def in &self.config.definitions {
            let value = preset.values.get(&def.id).copied().unwrap_or(def.default);
            values.push(AxisValue {
                axis_id: def.id.clone(),
                value: value.clamp(0.0, 1.0),
            });
        }

        CommandResult::ToneChange(values)
    }

    fn handle_set(&self, state: &GameSnapshot, args: &str) -> CommandResult {
        let (axis_id, value_str) = match args.split_once(' ') {
            Some((a, v)) => (a.trim(), v.trim()),
            None => {
                return CommandResult::Error(
                    "Usage: /tone set <axis_id> <value>".to_string(),
                );
            }
        };

        // Case-insensitive axis lookup
        let def = self.config.definitions.iter().find(|d| {
            d.id.eq_ignore_ascii_case(axis_id)
        });

        let Some(def) = def else {
            let ids: Vec<&str> = self.config.definitions.iter().map(|d| d.id.as_str()).collect();
            return CommandResult::Error(format!(
                "Unknown axis '{}'. Available: {}",
                axis_id,
                ids.join(", ")
            ));
        };

        let value: f64 = match value_str.parse() {
            Ok(v) => v,
            Err(_) => {
                return CommandResult::Error(format!(
                    "Invalid value '{}'. Must be a number between 0.0 and 1.0.",
                    value_str
                ));
            }
        };

        if !(0.0..=1.0).contains(&value) {
            return CommandResult::Error(format!(
                "Value {} is out of range. Must be between 0.0 and 1.0.",
                value
            ));
        }

        // Copy existing values, replacing the target axis
        let mut values: Vec<AxisValue> = state.axis_values.clone();
        let existing = values.iter_mut().find(|v| v.axis_id.eq_ignore_ascii_case(&def.id));
        if let Some(existing) = existing {
            existing.value = value;
        } else {
            values.push(AxisValue {
                axis_id: def.id.clone(),
                value,
            });
        }

        CommandResult::ToneChange(values)
    }
}

/// Resolve the current value for an axis, falling back to the definition default.
fn resolve_axis_value(values: &[AxisValue], def: &AxisDefinition) -> f64 {
    values
        .iter()
        .find(|v| v.axis_id.eq_ignore_ascii_case(&def.id))
        .map(|v| v.value)
        .unwrap_or(def.default)
}

/// Render a simple 10-char progress bar.
fn render_bar(value: f64) -> String {
    let filled = (value * 10.0).round() as usize;
    let empty = 10 - filled.min(10);
    format!("{}{}", "=".repeat(filled.min(10)), "-".repeat(empty))
}

/// Left pole indicator (highlighted when value < 0.35).
fn bar_left(value: f64) -> &'static str {
    if value < 0.35 { "*" } else { " " }
}

/// Right pole indicator (highlighted when value > 0.65).
fn bar_right(value: f64) -> &'static str {
    if value > 0.65 { "*" } else { " " }
}

/// Format tone context for injection into the narrator prompt.
///
/// Reads the current axis values from the game snapshot and the axis definitions
/// from the genre pack config. Produces a `[TONE]` block.
///
/// Modifier selection:
/// - value < 0.35: use low pole modifier (poles[0])
/// - value > 0.65: use high pole modifier (poles[1])
/// - 0.35-0.65: blend both poles
pub fn format_tone_context(config: &AxesConfig, axis_values: &[AxisValue]) -> String {
    if config.definitions.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    for def in &config.definitions {
        let value = resolve_axis_value(axis_values, def);

        // Look up modifier text for this axis
        let modifiers = config.modifiers.get(&def.id);
        let low_pole = def.poles.first().map(|s| s.as_str()).unwrap_or("low");
        let high_pole = def.poles.get(1).map(|s| s.as_str()).unwrap_or("high");

        if let Some(mods) = modifiers {
            if value < 0.35 {
                if let Some(text) = mods.get(low_pole) {
                    lines.push(format!("{} ({}): {}", def.name, low_pole, text));
                }
            } else if value > 0.65 {
                if let Some(text) = mods.get(high_pole) {
                    lines.push(format!("{} ({}): {}", def.name, high_pole, text));
                }
            } else {
                // Blend: include both modifiers
                let low_text = mods.get(low_pole);
                let high_text = mods.get(high_pole);
                match (low_text, high_text) {
                    (Some(lt), Some(ht)) => {
                        lines.push(format!(
                            "{} (balanced between {} and {}): {} / {}",
                            def.name, low_pole, high_pole, lt, ht
                        ));
                    }
                    (Some(lt), None) => {
                        lines.push(format!("{} ({}): {}", def.name, low_pole, lt));
                    }
                    (None, Some(ht)) => {
                        lines.push(format!("{} ({}): {}", def.name, high_pole, ht));
                    }
                    (None, None) => {}
                }
            }
        }
    }

    if lines.is_empty() {
        return String::new();
    }

    format!("\n[TONE]\n{}\n[/TONE]", lines.join("\n"))
}
