//! Aside handling — out-of-character commentary that skips state mutations.

use sidequest_agents::orchestrator::TurnContext;
use sidequest_protocol::{GameMessage, NarrationEndPayload, NarrationPayload};

use crate::extraction::{
    strip_combat_brackets, strip_fenced_blocks, strip_fourth_wall, strip_location_header,
};
use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Handle an aside — out-of-character commentary that does not affect the game world.
///
/// Calls the narrator with an aside-specific prompt injection, then returns narration
/// only. Skips ALL state mutation subsystems: no combat, no chase, no tropes, no
/// renders, no music, no NPC registry, no narration history, no turn barrier.
pub(super) async fn handle_aside(ctx: &mut DispatchContext<'_>) -> Vec<GameMessage> {
    tracing::info!(player = %ctx.char_name, action = %ctx.action, "aside — out-of-character, skipping state mutations");

    // Asides are out-of-character — no game state references, minimal prompt
    let aside_relevance = sidequest_game::PreprocessedAction {
        you: ctx.action.to_string(),
        named: ctx.action.to_string(),
        intent: ctx.action.to_string(),
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let mut state_summary = super::prompt::build_prompt_context(ctx, &aside_relevance).await;
    state_summary.push_str(concat!(
        "\n\nASIDE RULES (HARD CONSTRAINTS):",
        "\nThe player is speaking an aside — an out-of-character thought, whisper, or ",
        "meta-commentary. This is NOT an in-world action.",
        "\n1. Respond with a brief inner-monologue, fourth-wall-breaking quip, or flavor acknowledgment.",
        "\n2. Do NOT advance the story, trigger combat, move NPCs, or change ANY game state.",
        "\n3. Do NOT describe the character performing any actions or interacting with the world.",
        "\n4. Keep it short — 1-3 sentences maximum.",
    ));

    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: ctx.in_combat(),
        in_chase: ctx.in_chase(),
        in_encounter: ctx.in_encounter(),
        narrator_verbosity: ctx.narrator_verbosity,
        narrator_vocabulary: ctx.narrator_vocabulary,
        pending_trope_context: None,
        active_trope_summary: None,
        genre: Some(ctx.genre_slug.to_string()),
        available_sfx: ctx.sfx_library.keys().cloned().collect(),
        npc_registry: Vec::new(),
        npcs: Vec::new(),
        current_location: String::new(),
        world_graph: None,
        history_chapters: Vec::new(),
        campaign_maturity: sidequest_game::world_materialization::CampaignMaturity::default(),
        character_name: ctx.char_name.to_string(),
        genre_prompts: {
            let gs = ctx.genre_slug;
            sidequest_genre::GenreCode::new(gs)
                .ok()
                .and_then(|gc| ctx.state.genre_cache().get_or_load(&gc, ctx.state.genre_loader()).ok())
                .map(|pack| pack.prompts.clone())
        },
    };
    let result = ctx
        .state
        .game_service()
        .process_action(&format!("(aside) {}", ctx.action), &context);

    // Watcher: prompt assembled for aside (story 18-6)
    if let Some(ref zb) = result.zone_breakdown {
        let total_tokens: usize = zb.zones.iter().map(|z| z.total_tokens).sum();
        let section_count: usize = zb.zones.iter().map(|z| z.sections.len()).sum();
        WatcherEventBuilder::new("prompt", WatcherEventType::PromptAssembled)
            .field("agent", "narrator")
            .field("total_tokens", total_tokens)
            .field("section_count", section_count)
            .field("zones", &zb.zones)
            .field("full_prompt", &zb.full_prompt)
            .send();
    }

    let narration_text = strip_fourth_wall(&strip_combat_brackets(&strip_fenced_blocks(&strip_location_header(&result.narration))));

    vec![
        GameMessage::Narration {
            payload: NarrationPayload {
                text: narration_text.to_string(),
                state_delta: None,
                footnotes: vec![],
            },
            player_id: ctx.player_id.to_string(),
        },
        GameMessage::NarrationEnd {
            payload: NarrationEndPayload { state_delta: None },
            player_id: ctx.player_id.to_string(),
        },
    ]
}
