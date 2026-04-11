//! Sealed-round prompt context — composes all player actions + initiative data
//! into a single narrator prompt section.
//!
//! Story 13-14: After the barrier resolves, ONE narrator call processes all
//! simultaneous actions. This module builds the prompt context that tells the
//! narrator what happened and how to resolve it.

use std::collections::HashMap;

use sidequest_genre::InitiativeRule;

/// All context needed for a sealed-round narrator call.
///
/// Built from barrier actions + genre pack initiative rules + player stats.
/// Consumed by the dispatch layer to compose the narrator prompt.
#[derive(Debug, Clone)]
pub struct SealedRoundContext {
    /// Character name → action text (from barrier named_actions).
    actions: HashMap<String, String>,
    /// The encounter type driving this round (e.g., "combat", "social").
    encounter_type: String,
    /// Initiative rule for this encounter type, if the genre defines one.
    initiative_rule: Option<InitiativeRule>,
    /// Per-player stat values for the initiative stat, if available.
    /// Character name → stat value.
    initiative_stats: HashMap<String, i32>,
}

impl SealedRoundContext {
    /// Number of players who submitted actions.
    pub fn player_count(&self) -> usize {
        self.actions.len()
    }

    /// The encounter type for this round.
    pub fn encounter_type(&self) -> &str {
        &self.encounter_type
    }

    /// Number of actions collected.
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// Format as a prompt section for the narrator.
    ///
    /// Produces a self-contained block that tells the narrator:
    /// - All actions were submitted simultaneously (sealed-letter)
    /// - The encounter type and initiative context
    /// - Per-player stat values for determining initiative order
    /// - Instructions to synthesize one scene in third-person omniscient
    pub fn to_prompt_section(&self) -> String {
        let mut parts = Vec::new();

        // Header: simultaneous submission context
        parts.push(format!(
            "## Sealed Round — Simultaneous Actions ({})",
            self.encounter_type
        ));
        parts.push(String::new());
        parts.push(
            "All actions below were submitted simultaneously. \
             No player knew what others chose. \
             Resolve all actions in one synthesized scene using third-person omniscient perspective."
                .to_string(),
        );
        parts.push(String::new());

        // Actions as unordered set (bullet points, no numbers)
        parts.push("### Player Actions".to_string());
        for (name, action) in &self.actions {
            parts.push(format!("- {}: {}", name, action));
        }
        parts.push(String::new());

        // Initiative context (only if genre defines rules for this encounter type)
        if let Some(ref rule) = self.initiative_rule {
            parts.push("### Initiative Context".to_string());
            parts.push(format!(
                "Encounter type: {} — {}",
                self.encounter_type, rule.description
            ));
            parts.push(format!(
                "Primary stat for initiative order: {}",
                rule.primary_stat
            ));

            if !self.initiative_stats.is_empty() {
                parts.push(String::new());
                parts.push("Player stats:".to_string());
                for (name, value) in &self.initiative_stats {
                    parts.push(format!("- {} {}: {}", name, rule.primary_stat, value));
                }
            }

            parts.push(String::new());
            parts.push(
                "Determine initiative order based on the stats above, \
                 then narrate actions in that order."
                    .to_string(),
            );
        }

        parts.join("\n")
    }
}

/// Build a sealed-round context from barrier actions and genre pack data.
///
/// Called after barrier resolves by the claiming handler (the one that will
/// make the single narrator call).
pub fn build_sealed_round_context(
    actions: &HashMap<String, String>,
    encounter_type: &str,
    initiative_rules: &HashMap<String, InitiativeRule>,
    player_stats: &HashMap<String, HashMap<String, i32>>,
) -> SealedRoundContext {
    let initiative_rule = initiative_rules.get(encounter_type).cloned();

    // Extract the relevant stat value per player for the initiative stat
    let initiative_stats = if let Some(ref rule) = initiative_rule {
        actions
            .keys()
            .filter_map(|name| {
                player_stats
                    .get(name)
                    .and_then(|stats| stats.get(&rule.primary_stat))
                    .map(|&val| (name.clone(), val))
            })
            .collect()
    } else {
        HashMap::new()
    };

    SealedRoundContext {
        actions: actions.clone(),
        encounter_type: encounter_type.to_string(),
        initiative_rule,
        initiative_stats,
    }
}
