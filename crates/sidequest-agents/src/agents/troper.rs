//! Troper agent — trope beat injection into narrator context.
//!
//! When the TropeEngine fires escalation beats (progression past thresholds),
//! the Troper generates narrative instructions for weaving those beats into
//! the story naturally. These instructions are injected into the narrator's
//! context so trope progression feels organic, not mechanical. (ADR-018)

use std::collections::HashSet;

use crate::agent::Agent;
use crate::client::ClaudeClient;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};
use sidequest_game::trope::{FiredBeat, TropeState, TropeStatus};
use sidequest_genre::TropeDefinition;

/// System prompt for the Troper agent.
const TROPER_SYSTEM_PROMPT: &str = "\
<system>
You are the TROPER agent in SideQuest, a collaborative AI Dungeon Master.

Your role: translate mechanical trope beats into narrative instructions that the
Narrator can weave into the story naturally. You do NOT narrate directly — you
produce INSTRUCTIONS for the Narrator.

CORE PRINCIPLES:
- Trope beats should feel like organic story progression, not game mechanics.
- Show, don't tell. \"The innkeeper's hands tremble when she mentions the mines\"
  not \"The suspicion trope has escalated.\"
- Beats are suggestions with weight, not mandates. The Narrator integrates them.
- Multiple beats in one turn should be prioritized — lead with the highest-stakes beat.
- Resolution beats should feel earned, not arbitrary.

OUTPUT FORMAT:
Produce a numbered list of narrative directives, one per fired beat. Each directive has:
1. A one-sentence instruction for the Narrator (what to weave in)
2. NPCs to involve (if any)
3. Emotional tone (tension, relief, dread, wonder, etc.)

Example:
1. WEAVE: The guards at the gate exchange a meaningful look when the player approaches — seeds of organized resistance.
   INVOLVE: Gate Captain Voss
   TONE: tension, paranoia

Keep directives concise. The Narrator will expand them into prose.
</system>";

/// The Troper agent — trope beat injection into narrator context.
///
/// Unlike most agents, the Troper does not produce player-facing narration.
/// It generates narrative directives that get injected into the narrator's
/// prompt as situational context, ensuring trope beats are woven into the
/// story organically.
pub struct TroperAgent {
    system_prompt: String,
    /// Fired beats to inject into context this turn.
    fired_beats: Vec<FiredBeat>,
    /// Trope definitions for context enrichment.
    trope_definitions: Vec<TropeDefinition>,
    /// Active trope states for progression context.
    trope_states: Vec<TropeState>,
}

impl TroperAgent {
    /// Create a new Troper agent with no pending beats.
    pub fn new() -> Self {
        Self {
            system_prompt: TROPER_SYSTEM_PROMPT.to_string(),
            fired_beats: Vec::new(),
            trope_definitions: Vec::new(),
            trope_states: Vec::new(),
        }
    }

    /// Load fired beats for this turn's context injection.
    pub fn set_fired_beats(&mut self, beats: Vec<FiredBeat>) {
        self.fired_beats = beats;
    }

    /// Load trope definitions for context enrichment.
    pub fn set_trope_definitions(&mut self, defs: Vec<TropeDefinition>) {
        self.trope_definitions = defs;
    }

    /// Load active trope states for progression context.
    pub fn set_trope_states(&mut self, states: Vec<TropeState>) {
        self.trope_states = states;
    }

    /// Whether there are fired beats pending injection.
    pub fn has_pending_beats(&self) -> bool {
        !self.fired_beats.is_empty()
    }

    /// Evaluate narration against available tropes using LLM semantic matching.
    ///
    /// Replaces keyword substring matching with Claude-based evaluation.
    /// Returns a list of trope IDs that the LLM determined were triggered
    /// by this turn's narration. Unknown or already-active IDs are rejected.
    pub fn evaluate_triggers(
        client: &ClaudeClient,
        narration: &str,
        available_tropes: &[TropeDefinition],
        active_ids: &HashSet<String>,
    ) -> Vec<String> {
        let span = tracing::info_span!(
            "trope.evaluate_triggers",
            tropes_evaluated = tracing::field::Empty,
            narration_len = narration.len(),
            activations_returned = tracing::field::Empty,
        );
        let _guard = span.enter();

        // Filter to dormant tropes with triggers
        let candidates: Vec<&TropeDefinition> = available_tropes
            .iter()
            .filter(|td| {
                if let Some(ref id) = td.id {
                    !active_ids.contains(id) && !td.triggers.is_empty()
                } else {
                    false
                }
            })
            .collect();

        span.record("tropes_evaluated", candidates.len() as u64);

        if candidates.is_empty() {
            span.record("activations_returned", 0u64);
            return Vec::new();
        }

        // Build prompt listing available tropes and their triggers
        let mut prompt = String::from(
            "You evaluate whether narrative events trigger story tropes.\n\n\
             [AVAILABLE TROPES]\n",
        );
        for td in &candidates {
            let id = td.id.as_deref().unwrap();
            let triggers = td.triggers.join("; ");
            prompt.push_str(&format!("- {}: {}\n", id, triggers));
        }
        prompt.push_str(&format!(
            "\n[NARRATIVE THIS TURN]\n{}\n\n\
             Rules:\n\
             - Only activate tropes clearly triggered by THIS turn's narrative\n\
             - Do not speculatively activate tropes that might be triggered later\n\
             - Use exact trope IDs from the list above\n\n\
             Respond ONLY with JSON: {{\"trope_activations\": [\"id1\", \"id2\"]}}\n\
             If nothing triggers, respond: {{\"trope_activations\": []}}\n",
            narration,
        ));

        // Use haiku for speed — this is a classification task, not creative work
        let response = match client.send_with_model(&prompt, "claude-haiku-4-5-20251001") {
            Ok(resp) => resp.text,
            Err(e) => {
                tracing::error!(error = %e, "Trope trigger evaluation LLM call failed");
                span.record("activations_returned", 0u64);
                return Vec::new();
            }
        };

        // Parse JSON response
        let activations = Self::parse_trigger_response(&response, available_tropes, active_ids);
        span.record("activations_returned", activations.len() as u64);

        if !activations.is_empty() {
            tracing::info!(
                activations = ?activations,
                "LLM trope trigger evaluation returned activations"
            );
        }

        activations
    }

    /// Parse the LLM's JSON response for trope_activations, validating IDs.
    fn parse_trigger_response(
        response: &str,
        available_tropes: &[TropeDefinition],
        active_ids: &HashSet<String>,
    ) -> Vec<String> {
        // Try to extract JSON from response (may have markdown fences)
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        #[derive(serde::Deserialize)]
        struct TriggerResponse {
            #[serde(default)]
            trope_activations: Vec<String>,
        }

        let parsed: TriggerResponse = match serde_json::from_str(json_str) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    raw_response = %response,
                    "Failed to parse trope trigger evaluation response"
                );
                return Vec::new();
            }
        };

        let known_ids: HashSet<&str> = available_tropes
            .iter()
            .filter_map(|td| td.id.as_deref())
            .collect();

        parsed
            .trope_activations
            .into_iter()
            .filter(|id| {
                if !known_ids.contains(id.as_str()) {
                    tracing::warn!(trope_id = %id, "LLM returned unknown trope ID — skipping");
                    return false;
                }
                if active_ids.contains(id) {
                    tracing::debug!(trope_id = %id, "LLM returned already-active trope — skipping");
                    return false;
                }
                true
            })
            .collect()
    }

    /// Format a single fired beat into a context block.
    fn format_beat(beat: &FiredBeat, trope_def: Option<&TropeDefinition>) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "TROPE: {} (progression threshold: {:.0}%)",
            beat.trope_name,
            beat.beat.at * 100.0,
        ));

        parts.push(format!("BEAT EVENT: {}", beat.beat.event));

        if !beat.beat.stakes.is_empty() {
            parts.push(format!("STAKES: {}", beat.beat.stakes));
        }

        if !beat.beat.npcs_involved.is_empty() {
            parts.push(format!(
                "NPCs INVOLVED: {}",
                beat.beat.npcs_involved.join(", ")
            ));
        }

        // Enrich with trope definition context if available
        if let Some(def) = trope_def {
            if !def.narrative_hints.is_empty() {
                parts.push(format!(
                    "NARRATIVE HINTS: {}",
                    def.narrative_hints.join("; ")
                ));
            }
            if !def.tags.is_empty() {
                parts.push(format!("THEMES: {}", def.tags.join(", ")));
            }
        }

        parts.join("\n")
    }

    /// Build the full beats context block for injection into the narrator's prompt.
    pub fn build_beats_context(&self) -> Option<String> {
        if self.fired_beats.is_empty() {
            return None;
        }

        let def_map: std::collections::HashMap<&str, &TropeDefinition> = self
            .trope_definitions
            .iter()
            .filter_map(|td| td.id.as_deref().map(|id| (id, td)))
            .collect();

        let mut blocks: Vec<String> = Vec::new();

        for beat in &self.fired_beats {
            let trope_def = def_map.get(beat.trope_id.as_str()).copied();
            blocks.push(Self::format_beat(beat, trope_def));
        }

        // Add active trope progression summary for broader context
        let active_summary = self.build_progression_summary();

        let mut result = String::from(
            "[TROPE BEATS — MANDATORY WEAVE]\n\
             The following trope beats have fired this turn. The Narrator MUST\n\
             weave these into the narration naturally. Show, don't tell.\n\n",
        );

        for (i, block) in blocks.iter().enumerate() {
            result.push_str(&format!("--- Beat {} ---\n{}\n\n", i + 1, block));
        }

        if let Some(summary) = active_summary {
            result.push_str(&format!(
                "[ACTIVE TROPES — BACKGROUND CONTEXT]\n{}\n",
                summary
            ));
        }

        Some(result)
    }

    /// Build a summary of all active trope progression for background context.
    fn build_progression_summary(&self) -> Option<String> {
        let active: Vec<&TropeState> = self
            .trope_states
            .iter()
            .filter(|ts| {
                matches!(
                    ts.status(),
                    TropeStatus::Active | TropeStatus::Progressing
                )
            })
            .collect();

        if active.is_empty() {
            return None;
        }

        let def_map: std::collections::HashMap<&str, &TropeDefinition> = self
            .trope_definitions
            .iter()
            .filter_map(|td| td.id.as_deref().map(|id| (id, td)))
            .collect();

        let lines: Vec<String> = active
            .iter()
            .map(|ts| {
                let name = def_map
                    .get(ts.trope_definition_id())
                    .map(|d| d.name.as_str())
                    .unwrap_or(ts.trope_definition_id());
                format!(
                    "- {} [{:?}]: {:.0}% progressed",
                    name,
                    ts.status(),
                    ts.progression() * 100.0,
                )
            })
            .collect();

        Some(lines.join("\n"))
    }
}

impl Default for TroperAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for TroperAgent {
    fn name(&self) -> &str {
        "troper"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn build_context(&self, builder: &mut ContextBuilder) {
        // Agent identity (Primacy zone)
        builder.add_section(PromptSection::new(
            "troper_identity",
            self.system_prompt(),
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));

        // Fired beats (Early zone — high attention, this turn's beats)
        if let Some(beats_context) = self.build_beats_context() {
            builder.add_section(PromptSection::new(
                "troper_fired_beats",
                beats_context,
                AttentionZone::Early,
                SectionCategory::State,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use sidequest_genre::TropeEscalation;
    use sidequest_protocol::NonBlankString;

    fn make_beat(trope_id: &str, trope_name: &str, at: f64, event: &str) -> FiredBeat {
        FiredBeat {
            trope_id: trope_id.to_string(),
            trope_name: trope_name.to_string(),
            beat: TropeEscalation {
                at,
                event: event.to_string(),
                npcs_involved: vec![],
                stakes: String::new(),
            },
        }
    }

    fn make_beat_with_details(
        trope_id: &str,
        trope_name: &str,
        at: f64,
        event: &str,
        npcs: Vec<&str>,
        stakes: &str,
    ) -> FiredBeat {
        FiredBeat {
            trope_id: trope_id.to_string(),
            trope_name: trope_name.to_string(),
            beat: TropeEscalation {
                at,
                event: event.to_string(),
                npcs_involved: npcs.into_iter().map(String::from).collect(),
                stakes: stakes.to_string(),
            },
        }
    }

    fn make_trope_def(id: &str, name: &str) -> TropeDefinition {
        TropeDefinition {
            id: Some(id.to_string()),
            name: NonBlankString::new(name).unwrap(),
            description: None,
            category: String::new(),
            triggers: vec![],
            narrative_hints: vec![],
            tension_level: None,
            resolution_hints: None,
            resolution_patterns: None,
            tags: vec![],
            escalation: vec![],
            passive_progression: None,
            is_abstract: false,
            extends: None,
        }
    }

    #[test]
    fn new_troper_has_no_pending_beats() {
        let agent = TroperAgent::new();
        assert!(!agent.has_pending_beats());
        assert!(agent.build_beats_context().is_none());
    }

    #[test]
    fn troper_implements_agent_trait() {
        let agent = TroperAgent::new();
        assert_eq!(agent.name(), "troper");
        assert!(agent.system_prompt().contains("TROPER"));
    }

    #[test]
    fn default_matches_new() {
        let a = TroperAgent::new();
        let b = TroperAgent::default();
        assert_eq!(a.name(), b.name());
        assert_eq!(a.system_prompt(), b.system_prompt());
    }

    #[test]
    fn set_fired_beats_marks_pending() {
        let mut agent = TroperAgent::new();
        agent.set_fired_beats(vec![make_beat(
            "suspicion",
            "Suspicion",
            0.3,
            "Seeds of doubt",
        )]);
        assert!(agent.has_pending_beats());
    }

    #[test]
    fn build_beats_context_formats_single_beat() {
        let mut agent = TroperAgent::new();
        agent.set_fired_beats(vec![make_beat(
            "suspicion",
            "Suspicion",
            0.3,
            "Whispers begin among the townsfolk",
        )]);

        let ctx = agent.build_beats_context().unwrap();
        assert!(ctx.contains("TROPE BEATS"));
        assert!(ctx.contains("MANDATORY WEAVE"));
        assert!(ctx.contains("Suspicion"));
        assert!(ctx.contains("30%"));
        assert!(ctx.contains("Whispers begin among the townsfolk"));
    }

    #[test]
    fn build_beats_context_includes_stakes_and_npcs() {
        let mut agent = TroperAgent::new();
        agent.set_fired_beats(vec![make_beat_with_details(
            "rebellion",
            "Rebellion",
            0.5,
            "The resistance makes contact",
            vec!["Captain Voss", "The Informant"],
            "The city guard tightens patrols",
        )]);

        let ctx = agent.build_beats_context().unwrap();
        assert!(ctx.contains("STAKES: The city guard tightens patrols"));
        assert!(ctx.contains("Captain Voss"));
        assert!(ctx.contains("The Informant"));
    }

    #[test]
    fn build_beats_context_enriches_from_trope_defs() {
        let mut agent = TroperAgent::new();
        let mut def = make_trope_def("suspicion", "Suspicion");
        def.narrative_hints = vec!["NPCs avoid eye contact".to_string()];
        def.tags = vec!["paranoia".to_string(), "distrust".to_string()];

        agent.set_trope_definitions(vec![def]);
        agent.set_fired_beats(vec![make_beat(
            "suspicion",
            "Suspicion",
            0.3,
            "Seeds of doubt",
        )]);

        let ctx = agent.build_beats_context().unwrap();
        assert!(ctx.contains("NARRATIVE HINTS: NPCs avoid eye contact"));
        assert!(ctx.contains("THEMES: paranoia, distrust"));
    }

    #[test]
    fn build_beats_context_multiple_beats_numbered() {
        let mut agent = TroperAgent::new();
        agent.set_fired_beats(vec![
            make_beat("suspicion", "Suspicion", 0.3, "Seeds of doubt"),
            make_beat("rebellion", "Rebellion", 0.5, "Open defiance"),
        ]);

        let ctx = agent.build_beats_context().unwrap();
        assert!(ctx.contains("Beat 1"));
        assert!(ctx.contains("Beat 2"));
        assert!(ctx.contains("Suspicion"));
        assert!(ctx.contains("Rebellion"));
    }

    #[test]
    fn progression_summary_included_when_active_tropes_exist() {
        let mut agent = TroperAgent::new();
        let def = make_trope_def("suspicion", "Suspicion");
        let mut state = TropeState::new("suspicion");
        state.set_progression(0.45);

        agent.set_trope_definitions(vec![def]);
        agent.set_trope_states(vec![state]);
        agent.set_fired_beats(vec![make_beat(
            "suspicion",
            "Suspicion",
            0.3,
            "Seeds of doubt",
        )]);

        let ctx = agent.build_beats_context().unwrap();
        assert!(ctx.contains("ACTIVE TROPES"));
        assert!(ctx.contains("Suspicion"));
        assert!(ctx.contains("45%"));
    }

    #[test]
    fn progression_summary_excludes_resolved_tropes() {
        let mut agent = TroperAgent::new();
        let def = make_trope_def("old_news", "Old News");
        let mut state = TropeState::new("old_news");
        state.set_status(TropeStatus::Resolved);
        state.set_progression(1.0);

        agent.set_trope_definitions(vec![def]);
        agent.set_trope_states(vec![state]);
        agent.set_fired_beats(vec![make_beat("other", "Other", 0.1, "Something")]);

        let ctx = agent.build_beats_context().unwrap();
        // Resolved trope should NOT appear in progression summary
        assert!(!ctx.contains("Old News"));
    }

    #[test]
    fn build_context_adds_identity_and_beats_sections() {
        let mut agent = TroperAgent::new();
        agent.set_fired_beats(vec![make_beat(
            "suspicion",
            "Suspicion",
            0.3,
            "Seeds of doubt",
        )]);

        let mut builder = ContextBuilder::new();
        agent.build_context(&mut builder);

        let sections = builder.build();
        assert_eq!(sections.len(), 2);
        // Primacy section first (identity)
        assert_eq!(sections[0].zone, AttentionZone::Primacy);
        // Early section second (fired beats)
        assert_eq!(sections[1].zone, AttentionZone::Early);
        assert!(sections[1].content.contains("TROPE BEATS"));
    }

    #[test]
    fn build_context_omits_beats_section_when_no_beats() {
        let agent = TroperAgent::new();
        let mut builder = ContextBuilder::new();
        agent.build_context(&mut builder);

        let sections = builder.build();
        // Only identity section, no beats
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].zone, AttentionZone::Primacy);
    }

    // ========================================================================
    // Trigger evaluation parsing tests
    // ========================================================================

    fn make_trope_def_with_triggers(id: &str, name: &str, triggers: Vec<&str>) -> TropeDefinition {
        let mut def = make_trope_def(id, name);
        def.triggers = triggers.into_iter().map(String::from).collect();
        def
    }

    #[test]
    fn parse_trigger_response_valid_json() {
        let defs = vec![
            make_trope_def_with_triggers("inquisition", "Inquisition", vec!["magic"]),
            make_trope_def_with_triggers("heir", "Heir", vec!["bloodline"]),
        ];
        let active: HashSet<String> = HashSet::new();

        let result = TroperAgent::parse_trigger_response(
            r#"{"trope_activations": ["inquisition"]}"#,
            &defs,
            &active,
        );

        assert_eq!(result, vec!["inquisition"]);
    }

    #[test]
    fn parse_trigger_response_empty_activations() {
        let defs = vec![make_trope_def_with_triggers("x", "X", vec!["y"])];
        let active: HashSet<String> = HashSet::new();

        let result = TroperAgent::parse_trigger_response(
            r#"{"trope_activations": []}"#,
            &defs,
            &active,
        );

        assert!(result.is_empty());
    }

    #[test]
    fn parse_trigger_response_rejects_unknown_ids() {
        let defs = vec![make_trope_def_with_triggers("known", "Known", vec!["test"])];
        let active: HashSet<String> = HashSet::new();

        let result = TroperAgent::parse_trigger_response(
            r#"{"trope_activations": ["known", "totally_fake"]}"#,
            &defs,
            &active,
        );

        assert_eq!(result, vec!["known"]);
    }

    #[test]
    fn parse_trigger_response_rejects_already_active() {
        let defs = vec![
            make_trope_def_with_triggers("a", "A", vec!["x"]),
            make_trope_def_with_triggers("b", "B", vec!["y"]),
        ];
        let active: HashSet<String> = ["a".to_string()].into();

        let result = TroperAgent::parse_trigger_response(
            r#"{"trope_activations": ["a", "b"]}"#,
            &defs,
            &active,
        );

        assert_eq!(result, vec!["b"]);
    }

    #[test]
    fn parse_trigger_response_handles_markdown_fenced_json() {
        let defs = vec![make_trope_def_with_triggers("x", "X", vec!["y"])];
        let active: HashSet<String> = HashSet::new();

        let result = TroperAgent::parse_trigger_response(
            "```json\n{\"trope_activations\": [\"x\"]}\n```",
            &defs,
            &active,
        );

        assert_eq!(result, vec!["x"]);
    }

    #[test]
    fn parse_trigger_response_handles_garbage() {
        let defs = vec![make_trope_def_with_triggers("x", "X", vec!["y"])];
        let active: HashSet<String> = HashSet::new();

        let result =
            TroperAgent::parse_trigger_response("I think maybe the tropes...", &defs, &active);

        assert!(result.is_empty());
    }
}
