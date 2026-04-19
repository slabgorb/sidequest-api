//! Game state persistence — snapshot sync and SQLite save.

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Sync mutable dispatch locals into the canonical GameSnapshot.
pub(super) fn sync_locals_to_snapshot(ctx: &mut DispatchContext<'_>, _narration_text: &str) {
    // Use ctx.current_location (authoritative after room-graph validation in story 19-2)
    // instead of re-extracting from narration text, which would bypass validation.
    ctx.snapshot.location = ctx.current_location.clone();
    ctx.snapshot.turn_manager = ctx.turn_manager.clone();
    ctx.snapshot.npc_registry = ctx.npc_registry.clone();
    // Sync NPC HP from npc_registry back to snapshot.npcs so combat damage persists.
    // Without this, snapshot.npcs retains stale HP values and enemy damage resets
    // between turns (the HP changes only live in npc_registry during the turn).
    for entry in ctx.npc_registry.iter() {
        if let Some(npc) = ctx
            .snapshot
            .npcs
            .iter_mut()
            .find(|n| n.core.name.as_str().eq_ignore_ascii_case(&entry.name))
        {
            // Story 39-2: NpcRegistryEntry still carries legacy hp/max_hp for
            // the UI wire format (39-7 renames). Route through the npc's edge
            // pool so the GM panel + snapshot stay consistent.
            npc.core.edge.current = entry.hp;
            npc.core.edge.max = entry.max_hp;
        }
    }
    ctx.snapshot.genie_wishes = ctx.genie_wishes.clone();
    ctx.snapshot.axis_values = ctx.axis_values.clone();
    // combat/chase sync removed in story 28-9 — encounter is maintained directly via apply_beat().

    ctx.snapshot.discovered_regions = ctx.discovered_regions.clone();
    ctx.snapshot.active_tropes = ctx.trope_states.clone();
    ctx.snapshot.achievement_tracker = ctx.achievement_tracker.clone();
    ctx.snapshot.quest_log = ctx.quest_log.clone();
    // Phase 5: snapshot.resources is mutated in-place by patch + decay,
    // no end-of-turn sync required.
    if let Some(ref cj) = ctx.character_json {
        if let Ok(ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            if let Some(saved_ch) = ctx.snapshot.characters.first_mut() {
                saved_ch.core.edge.current = *ctx.edge;
                saved_ch.core.edge.max = *ctx.max_edge;
                saved_ch.core.level = *ctx.level;
                saved_ch.core.inventory = ctx.inventory.clone();
                saved_ch.known_facts = ch.known_facts.clone();
                saved_ch.affinities = ch.affinities.clone();
                saved_ch.narrative_state = ch.narrative_state.clone();
            }
        }
    }
}

/// Persist game state — save the canonical snapshot directly (no load round-trip).
///
/// Story 15-8: The old implementation loaded from SQLite on every turn just to
/// merge scattered locals, then saved. Now ctx.snapshot is synced before this
/// call, so we save directly — one round-trip instead of two.
pub(super) async fn persist_game_state(
    ctx: &mut DispatchContext<'_>,
    _narration_text: &str,
    clean_narration: &str,
) {
    if ctx.genre_slug.is_empty() || ctx.world_slug.is_empty() {
        tracing::debug!("persist_game_state skipped — empty genre or world slug");
        return;
    }

    // Append the current narration entry to ctx.snapshot and persist to narrative_log table
    let narrative_entry = sidequest_game::NarrativeEntry {
        timestamp: 0,
        round: ctx.turn_manager.interaction() as u32,
        author: "narrator".to_string(),
        content: clean_narration.to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    };
    ctx.snapshot.narrative_log.push(narrative_entry.clone());

    // Write to append-only narrative_log table in SQLite
    match ctx
        .state
        .persistence()
        .append_narrative(
            ctx.genre_slug,
            ctx.world_slug,
            ctx.player_name_for_save,
            &narrative_entry,
        )
        .await
    {
        Ok(()) => {
            WatcherEventBuilder::new("persistence", WatcherEventType::SubsystemExerciseSummary)
                .field("event", "persistence.narrative_appended")
                .field("turn", ctx.turn_manager.interaction())
                .field("length", clean_narration.len())
                .field("player", ctx.player_name_for_save)
                .send();
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to append narrative log entry");
        }
    }

    // Emit encounter OTEL event if active
    if let Some(ref enc) = ctx.snapshot.encounter {
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("encounter_type", &enc.encounter_type)
            .field("beat", enc.beat)
            .field("metric_name", &enc.metric.name)
            .field("metric_current", enc.metric.current)
            .field(
                "metric_threshold",
                enc.metric.threshold_high.or(enc.metric.threshold_low),
            )
            .field("phase", enc.structured_phase.map(|p| format!("{:?}", p)))
            .field("resolved", enc.resolved)
            .field("actor_count", enc.actors.len())
            .field_opt("mood_override", &enc.mood_override)
            .field_opt("outcome", &enc.outcome)
            .send();
    }

    // Save ctx.snapshot directly — no load round-trip needed (story 15-8)
    let start = std::time::Instant::now();
    match ctx
        .state
        .persistence()
        .save(
            ctx.genre_slug,
            ctx.world_slug,
            ctx.player_name_for_save,
            ctx.snapshot,
        )
        .await
    {
        Ok(_) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                player = %ctx.player_name_for_save,
                turn = ctx.turn_manager.interaction(),
                location = %ctx.current_location,
                ctx.edge = *ctx.edge,
                items = ctx.inventory.items.len(),
                save_latency_ms = elapsed_ms,
                "session.saved — game state persisted"
            );
            // OTEL: persistence save latency for GM panel verification
            WatcherEventBuilder::new("persistence", WatcherEventType::SubsystemExerciseSummary)
                .field("save_latency_ms", elapsed_ms)
                .field("player", ctx.player_name_for_save)
                .field("turn", ctx.turn_manager.interaction())
                .send();

            // NOTE: append_narrative is already called above (line ~2358) right
            // after the entry is created.  A duplicate call here was causing
            // every narration row to be written twice, which produced repeated
            // paragraphs in the "Previously On..." recap on session resume.
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to persist game state");
            WatcherEventBuilder::new("persistence", WatcherEventType::ValidationWarning)
                .field("event", "persistence.save_failed")
                .field("error", format!("{e}"))
                .field("player", ctx.player_name_for_save)
                .field("turn", ctx.turn_manager.interaction())
                .send();
        }
    }
}
