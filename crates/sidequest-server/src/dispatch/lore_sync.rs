//! Lore accumulation, persistence, and continuity validation.

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Accumulate a lore fragment and persist to SQLite + generate embedding.
pub(super) async fn accumulate_and_persist_lore(
    ctx: &mut DispatchContext<'_>,
    text: &str,
    category: sidequest_game::lore::LoreCategory,
    turn: u64,
    metadata: std::collections::HashMap<String, String>,
) -> Option<String> {
    match sidequest_game::accumulate_lore(
        ctx.lore_store,
        text,
        category.clone(),
        turn,
        metadata.clone(),
    ) {
        Ok(fragment_id) => {
            let category_str = category.to_string();
            let token_estimate = text.len().div_ceil(4);
            WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                .field("event", "lore.fragment_accumulated")
                .field("fragment_id", &fragment_id)
                .field("category", &category_str)
                .field("turn", turn)
                .field("token_estimate", token_estimate)
                .send();
            tracing::info!(
                fragment_id = %fragment_id,
                category = %category_str,
                turn = turn,
                token_estimate = token_estimate,
                "lore.fragment_accumulated"
            );

            let persist_fragment = sidequest_game::LoreFragment::new(
                fragment_id.clone(),
                category,
                text.to_string(),
                sidequest_game::LoreSource::GameEvent,
                Some(turn),
                metadata,
            );
            match ctx
                .state
                .persistence()
                .append_lore_fragment(
                    ctx.genre_slug,
                    ctx.world_slug,
                    ctx.player_name_for_save,
                    &persist_fragment,
                )
                .await
            {
                Ok(()) => {
                    WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                        .field("event", "lore.fragment_persisted")
                        .field("fragment_id", &fragment_id)
                        .field("category", &category_str)
                        .send();
                    tracing::info!(fragment_id = %fragment_id, "lore.fragment_persisted");
                }
                Err(e) => {
                    tracing::warn!(error = %e, fragment_id = %fragment_id, "lore.fragment_persist_failed");
                }
            }

            let config = sidequest_daemon_client::DaemonConfig::default();
            if let Ok(mut client) = sidequest_daemon_client::DaemonClient::connect(config).await {
                let embed_params = sidequest_daemon_client::EmbedParams {
                    text: text.to_string(),
                };
                match client.embed(embed_params).await {
                    Ok(embed_result) => {
                        if let Err(e) = ctx
                            .lore_store
                            .set_embedding(&fragment_id, embed_result.embedding)
                        {
                            tracing::warn!(error = %e, fragment_id = %fragment_id, "lore.embedding_attach_failed");
                        } else {
                            WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                                .field("event", "lore.embedding_generated")
                                .field("fragment_id", &fragment_id)
                                .field("latency_ms", embed_result.latency_ms)
                                .field("model", &embed_result.model)
                                .send();
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            fragment_id = %fragment_id,
                            "lore.embedding_generation_failed — fragment stored without embedding"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    fragment_id = %fragment_id,
                    "lore.daemon_connect_failed — fragment stored without embedding"
                );
            }

            Some(fragment_id)
        }
        Err(e) => {
            tracing::warn!(error = %e, "lore.accumulate_failed");
            None
        }
    }
}

/// Continuity validation — LLM-based check of narrator output against game state.
///
/// Uses Sonnet classification to detect contradictions rather than keyword matching.
/// Runs via spawn_blocking so it doesn't block the tokio runtime.
pub(super) async fn validate_continuity(ctx: &mut DispatchContext<'_>, clean_narration: &str) {
    let dead_npcs: Vec<String> = ctx
        .npc_registry
        .iter()
        .filter(|n| n.max_hp > 0 && n.hp <= 0)
        .map(|n| n.name.clone())
        .collect();

    let inventory_items: Vec<String> = ctx
        .inventory
        .carried()
        .map(|i| i.name.as_str().to_string())
        .collect();

    let character_description: String = ctx
        .character_json
        .as_ref()
        .and_then(|cj| cj.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    let validation_result = sidequest_agents::continuity_validator::validate_continuity_llm_async(
        clean_narration,
        &ctx.current_location,
        &dead_npcs,
        &inventory_items,
        "", // time_of_day not tracked in dispatch context yet
        &character_description,
    )
    .await;

    if !validation_result.is_clean() {
        let corrections = validation_result.format_corrections();
        tracing::warn!(
            contradictions = validation_result.contradictions.len(),
            "continuity.contradictions_detected"
        );
        for c in &validation_result.contradictions {
            tracing::warn!(
                category = ?c.category,
                detail = %c.detail,
                expected = %c.expected,
                "continuity.contradiction"
            );
        }
        *ctx.continuity_corrections = corrections;
    }
}
