//! Lore accumulation, persistence, and continuity validation.
//!
//! ## Embedding lives on a background worker
//!
//! Before the 2026-04-11 playtest fix this module awaited
//! `DaemonClient::embed()` synchronously in the dispatch critical path,
//! once per new fragment, plus a full retry sweep over all pending
//! fragments at the start of every call. When the daemon embed endpoint
//! wedged, a turn that generated N lore events with M pending fragments
//! incurred N × M × 10s of blocking.
//!
//! Embedding work now runs on the background worker defined in
//! `lore_embed_worker.rs`. Dispatch fires and forgets via an unbounded
//! mpsc channel; the worker processes requests serially, holds the embed
//! call outside the lore store lock, and trips a circuit breaker after
//! sustained failures so a sick daemon can't DoS the game loop.

use crate::{WatcherEventBuilder, WatcherEventType};

use super::lore_embed_worker::EmbedRequest;
use super::DispatchContext;

/// Accumulate a lore fragment and persist to SQLite. Embedding is kicked
/// off asynchronously on the background worker — this function never
/// awaits the daemon.
pub(super) async fn accumulate_and_persist_lore(
    ctx: &mut DispatchContext<'_>,
    text: &str,
    category: sidequest_game::lore::LoreCategory,
    turn: u64,
    metadata: std::collections::HashMap<String, String>,
) -> Option<String> {
    let fragment_id = {
        let mut store = ctx.lore_store.lock().await;
        // Explicit deref: pass a `&mut LoreStore` through the MutexGuard
        // rather than relying on autoref-to-guard coercion, which doesn't
        // bridge from `&mut MutexGuard<T>` to `&mut T` at call sites.
        match sidequest_game::accumulate_lore(
            &mut *store,
            text,
            category.clone(),
            turn,
            metadata.clone(),
        ) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(error = %e, "lore.accumulate_failed");
                return None;
            }
        }
    };

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

    // Persist the fragment to SQLite so save/restore picks it up even if
    // the embed worker hasn't run yet. This is a fast local write through
    // the persistence actor — no network, no lore store lock held.
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

    // Hand the fragment off to the background embed worker. This is a
    // non-blocking send on an unbounded channel — dispatch never awaits
    // the daemon's embed call. If the send fails the channel is closed
    // (session shutting down), so there's nothing useful to do; log and
    // move on.
    let req = EmbedRequest {
        fragment_id: fragment_id.clone(),
        text: text.to_string(),
    };
    if let Err(e) = ctx.lore_embed_tx.send(req) {
        tracing::warn!(
            error = %e,
            fragment_id = %fragment_id,
            "lore.embed_worker_send_failed — channel closed, embedding deferred to next session's initial sweep"
        );
    }

    Some(fragment_id)
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
