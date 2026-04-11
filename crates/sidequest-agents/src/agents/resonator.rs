//! Resonator agent — character perception, hook refinement, ability filtering.
//!
//! Ported from sq-2/sidequest/agents/hook_refiner.py + perception_rewriter.py.
//!
//! The Resonator combines two Python-era responsibilities:
//! 1. **Hook refinement** — polishes character narrative hooks (origin, wound,
//!    relationship, goal, trait, debt, secret, possession) so they stay
//!    consistent and dramatically useful as the story evolves.
//! 2. **Perception rewriting** — rewrites narrator output so each character
//!    perceives scenes through their unique lens (OCEAN personality, hooks,
//!    active perceptual effects like blinded/charmed/dominated).

use crate::agent::Agent;
use crate::client::ClaudeClient;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};
use sidequest_game::perception::{
    PerceptionFilter, PerceptionRewriter, RewriteStrategy, RewriterError,
};

/// System prompt for the Resonator agent.
const RESONATOR_SYSTEM_PROMPT: &str = "\
<system>
You are the RESONATOR agent in SideQuest, a collaborative AI Dungeon Master engine.

Your role: rewrite narration through the perceptual lens of a specific character.
Each character experiences the world differently based on their personality, hooks,
and active status effects.

REWRITE RULES:
- You receive base narration (what objectively happened) and a character profile.
- Rewrite the narration so it reads as that character's subjective experience.
- High Neuroticism: notice threats, read hostility into neutral situations, fixate on danger.
- High Openness: see beauty in decay, notice patterns, wonder at the strange.
- Low Agreeableness: read others' actions as self-serving, notice power dynamics.
- High Extraversion: focus on social cues, body language, who's talking to whom.
- Low Openness: dismiss the unfamiliar, focus on the practical and concrete.
- Hooks shape perception: a character with a \"wound: betrayal by a mentor\" will
  read authority figures with suspicion. A \"goal: find the lost artifact\" character
  notices any object that could be a clue.
- Active perceptual effects (Blinded, Charmed, Deafened, etc.) physically alter
  what the character can perceive — apply these as hard constraints, not flavoring.

OUTPUT:
- Return ONLY the rewritten narration. No meta-commentary, no JSON, no OOC text.
- Preserve the same approximate length as the base narration.
- Preserve all factual content — change perspective and emphasis, not events.
- Write in the same tense and person as the base narration.
</system>";

/// The Resonator agent — character perception, hook refinement.
pub struct ResonatorAgent {
    system_prompt: String,
}

impl ResonatorAgent {
    /// Create a new Resonator agent.
    pub fn new() -> Self {
        Self {
            system_prompt: RESONATOR_SYSTEM_PROMPT.to_string(),
        }
    }

    /// Build a perception rewrite prompt for a specific character.
    ///
    /// Composes the character's hooks, OCEAN summary, active perceptual effects,
    /// and the base narration into a single prompt string for Claude.
    pub fn build_rewrite_prompt(
        &self,
        base_narration: &str,
        character_name: &str,
        hooks: &[String],
        ocean_summary: &str,
        effect_description: &str,
        genre_voice: &str,
    ) -> String {
        let mut prompt = String::with_capacity(1024);

        prompt.push_str(&format!("[GENRE VOICE: {genre_voice}]\n\n"));

        prompt.push_str(&format!("[CHARACTER: {character_name}]\n"));

        if !hooks.is_empty() {
            prompt.push_str("[HOOKS]\n");
            for hook in hooks {
                prompt.push_str(&format!("- {hook}\n"));
            }
            prompt.push('\n');
        }

        if !ocean_summary.is_empty() {
            prompt.push_str(&format!("[PERSONALITY: {ocean_summary}]\n\n"));
        }

        if effect_description != "none" && !effect_description.is_empty() {
            prompt.push_str(&format!(
                "[ACTIVE PERCEPTUAL EFFECTS: {effect_description}]\n\n"
            ));
        }

        prompt.push_str("[BASE NARRATION]\n");
        prompt.push_str(base_narration);
        prompt.push_str("\n\n[REWRITE the above narration through this character's perception.]");

        prompt
    }
}

impl Default for ResonatorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for ResonatorAgent {
    fn name(&self) -> &str {
        "resonator"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn build_context(&self, builder: &mut ContextBuilder) {
        // Identity section in Primacy zone
        builder.add_section(PromptSection::new(
            "resonator_identity".to_string(),
            self.system_prompt(),
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
    }
}

/// A `RewriteStrategy` that calls Claude via the CLI subprocess to rewrite
/// narration through a character's perceptual lens.
///
/// Uses the Resonator agent's system prompt + a composed context prompt.
pub struct ClaudeRewriteStrategy {
    client: ClaudeClient,
    agent: ResonatorAgent,
}

impl ClaudeRewriteStrategy {
    /// Create a new Claude-backed rewrite strategy.
    pub fn new(client: ClaudeClient) -> Self {
        Self {
            client,
            agent: ResonatorAgent::new(),
        }
    }
}

impl RewriteStrategy for ClaudeRewriteStrategy {
    fn rewrite(
        &self,
        base_narration: &str,
        filter: &PerceptionFilter,
        genre_voice: &str,
    ) -> Result<String, RewriterError> {
        let effect_description = PerceptionRewriter::describe_effects(filter.effects());

        let prompt = self.agent.build_rewrite_prompt(
            base_narration,
            filter.character_name(),
            &[], // hooks come from character state, injected by caller
            "",  // OCEAN summary injected by caller
            &effect_description,
            genre_voice,
        );

        let full_prompt = format!("{}\n\n{}", self.agent.system_prompt(), prompt);

        self.client
            .send(&full_prompt)
            .map(|r| r.text)
            .map_err(|e| RewriterError::Agent(e.to_string()))
    }
}

/// A `RewriteStrategy` that calls Claude with full character context
/// (hooks + OCEAN + effects). Use this when you have the character data available.
pub struct FullContextRewriteStrategy {
    client: ClaudeClient,
    agent: ResonatorAgent,
    character_name: String,
    hooks: Vec<String>,
    ocean_summary: String,
}

impl FullContextRewriteStrategy {
    /// Create a strategy with full character context for perception rewriting.
    pub fn new(
        client: ClaudeClient,
        character_name: String,
        hooks: Vec<String>,
        ocean_summary: String,
    ) -> Self {
        Self {
            client,
            agent: ResonatorAgent::new(),
            character_name,
            hooks,
            ocean_summary,
        }
    }
}

impl RewriteStrategy for FullContextRewriteStrategy {
    fn rewrite(
        &self,
        base_narration: &str,
        filter: &PerceptionFilter,
        genre_voice: &str,
    ) -> Result<String, RewriterError> {
        let effect_description = PerceptionRewriter::describe_effects(filter.effects());

        let prompt = self.agent.build_rewrite_prompt(
            base_narration,
            &self.character_name,
            &self.hooks,
            &self.ocean_summary,
            &effect_description,
            genre_voice,
        );

        let full_prompt = format!("{}\n\n{}", self.agent.system_prompt(), prompt);

        self.client
            .send(&full_prompt)
            .map(|r| r.text)
            .map_err(|e| RewriterError::Agent(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resonator_name() {
        let agent = ResonatorAgent::new();
        assert_eq!(agent.name(), "resonator");
    }

    #[test]
    fn resonator_system_prompt_not_empty() {
        let agent = ResonatorAgent::new();
        assert!(!agent.system_prompt().is_empty());
    }

    #[test]
    fn resonator_system_prompt_has_system_tags() {
        let agent = ResonatorAgent::new();
        assert!(agent.system_prompt().contains("<system>"));
        assert!(agent.system_prompt().contains("</system>"));
    }

    #[test]
    fn resonator_system_prompt_mentions_perception() {
        let agent = ResonatorAgent::new();
        let prompt = agent.system_prompt();
        assert!(
            prompt.contains("perception") || prompt.contains("perceptual"),
            "system prompt should mention perception"
        );
    }

    #[test]
    fn resonator_system_prompt_mentions_ocean_traits() {
        let agent = ResonatorAgent::new();
        let prompt = agent.system_prompt();
        assert!(prompt.contains("Neuroticism"), "should mention Neuroticism");
        assert!(prompt.contains("Openness"), "should mention Openness");
    }

    #[test]
    fn resonator_system_prompt_mentions_hooks() {
        let agent = ResonatorAgent::new();
        let prompt = agent.system_prompt();
        assert!(
            prompt.contains("hook") || prompt.contains("Hook"),
            "system prompt should mention hooks"
        );
    }

    #[test]
    fn resonator_default_equivalent_to_new() {
        let from_new = ResonatorAgent::new();
        let from_default = ResonatorAgent::default();
        assert_eq!(from_new.name(), from_default.name());
        assert_eq!(from_new.system_prompt(), from_default.system_prompt());
    }

    #[test]
    fn resonator_builds_context_with_identity_section() {
        let agent = ResonatorAgent::new();
        let mut builder = ContextBuilder::new();
        agent.build_context(&mut builder);
        let sections = builder.sections_by_category(SectionCategory::Identity);
        assert_eq!(sections.len(), 1, "should add exactly one identity section");
    }

    #[test]
    fn build_rewrite_prompt_includes_genre_voice() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "The dragon roars.",
            "Thorn",
            &[],
            "",
            "none",
            "dark fantasy",
        );
        assert!(prompt.contains("dark fantasy"));
    }

    #[test]
    fn build_rewrite_prompt_includes_character_name() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "The dragon roars.",
            "Thorn Ironhide",
            &[],
            "",
            "none",
            "fantasy",
        );
        assert!(prompt.contains("Thorn Ironhide"));
    }

    #[test]
    fn build_rewrite_prompt_includes_hooks() {
        let agent = ResonatorAgent::new();
        let hooks = vec![
            "wound: betrayal by a mentor".to_string(),
            "goal: find the lost artifact".to_string(),
        ];
        let prompt =
            agent.build_rewrite_prompt("The elder speaks.", "Thorn", &hooks, "", "none", "fantasy");
        assert!(prompt.contains("betrayal by a mentor"));
        assert!(prompt.contains("find the lost artifact"));
    }

    #[test]
    fn build_rewrite_prompt_includes_ocean_summary() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "The room is quiet.",
            "Elara",
            &[],
            "anxious and volatile, curious and imaginative",
            "none",
            "horror",
        );
        assert!(prompt.contains("anxious and volatile"));
        assert!(prompt.contains("curious and imaginative"));
    }

    #[test]
    fn build_rewrite_prompt_includes_effects() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "Swords clash.",
            "Thorn",
            &[],
            "",
            "Blinded (cannot see); Deafened (cannot hear)",
            "dark fantasy",
        );
        assert!(prompt.contains("Blinded (cannot see)"));
        assert!(prompt.contains("Deafened (cannot hear)"));
    }

    #[test]
    fn build_rewrite_prompt_omits_effects_section_when_none() {
        let agent = ResonatorAgent::new();
        let prompt =
            agent.build_rewrite_prompt("The tavern is warm.", "Thorn", &[], "", "none", "fantasy");
        assert!(
            !prompt.contains("ACTIVE PERCEPTUAL EFFECTS"),
            "should omit effects section when 'none'"
        );
    }

    #[test]
    fn build_rewrite_prompt_includes_base_narration() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "The ancient dragon descends upon the ruined keep.",
            "Thorn",
            &[],
            "",
            "none",
            "fantasy",
        );
        assert!(prompt.contains("The ancient dragon descends upon the ruined keep."));
    }

    #[test]
    fn build_rewrite_prompt_ends_with_instruction() {
        let agent = ResonatorAgent::new();
        let prompt = agent.build_rewrite_prompt(
            "The door creaks open.",
            "Thorn",
            &[],
            "",
            "none",
            "fantasy",
        );
        assert!(prompt.contains("REWRITE"));
    }

    #[test]
    fn claude_rewrite_strategy_constructs() {
        let client = ClaudeClient::new();
        let _strategy = ClaudeRewriteStrategy::new(client);
    }

    #[test]
    fn full_context_rewrite_strategy_constructs() {
        let client = ClaudeClient::new();
        let _strategy = FullContextRewriteStrategy::new(
            client,
            "Thorn".to_string(),
            vec!["nemesis: The Warden".to_string()],
            "anxious and volatile".to_string(),
        );
    }

    #[test]
    fn claude_rewrite_strategy_implements_rewrite_strategy_trait() {
        // Verify trait object compatibility
        fn _accepts_strategy(_s: &dyn RewriteStrategy) {}
        let client = ClaudeClient::new();
        let strategy = ClaudeRewriteStrategy::new(client);
        _accepts_strategy(&strategy);
    }

    #[test]
    fn full_context_strategy_implements_rewrite_strategy_trait() {
        fn _accepts_strategy(_s: &dyn RewriteStrategy) {}
        let client = ClaudeClient::new();
        let strategy = FullContextRewriteStrategy::new(
            client,
            "Thorn".to_string(),
            vec![],
            "balanced temperament".to_string(),
        );
        _accepts_strategy(&strategy);
    }

    #[test]
    fn build_rewrite_prompt_full_context() {
        let agent = ResonatorAgent::new();
        let hooks = vec![
            "origin: exile from the northern clans".to_string(),
            "wound: lost their child to the plague".to_string(),
            "secret: knows the location of the vault".to_string(),
        ];
        let prompt = agent.build_rewrite_prompt(
            "The stranger approaches with an outstretched hand.",
            "Kael",
            &hooks,
            "reserved and quiet, anxious and volatile",
            "Charmed by Siren (perceives as trusted ally)",
            "dark fantasy",
        );
        // All elements present
        assert!(prompt.contains("Kael"));
        assert!(prompt.contains("exile from the northern clans"));
        assert!(prompt.contains("lost their child to the plague"));
        assert!(prompt.contains("knows the location of the vault"));
        assert!(prompt.contains("reserved and quiet"));
        assert!(prompt.contains("Charmed by Siren"));
        assert!(prompt.contains("dark fantasy"));
        assert!(prompt.contains("The stranger approaches"));
    }
}
