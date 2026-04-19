//! Response message construction — narration, party status, inventory, map, encounters.

use sidequest_protocol::{
    GameMessage, MapUpdatePayload, NarrationEndPayload, NarrationPayload, NonBlankString,
    PartyMember, PartyStatusPayload,
};

use crate::scrapbook;
use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Convert a dispatch-context string (player_id / player_name / location) to a
/// `NonBlankString`. These call sites all flow from session initialization or
/// genre-pack binding, where the upstream invariant is that the string is
/// non-empty — but we still fail loud (not silently fall back) so any future
/// regression surfaces at the construction site instead of drifting.
fn nbs_ctx(value: &str, what: &'static str) -> NonBlankString {
    NonBlankString::new(value)
        .unwrap_or_else(|_| panic!("{what} must be non-blank at dispatch time"))
}

/// Per-turn data passed to `build_response_messages` alongside the
/// long-lived `DispatchContext`. Bundles the turn-specific values that were
/// previously individual parameters (story 36-2).
pub(super) struct ResponseContext<'a> {
    pub clean_narration: &'a str,
    pub result: &'a sidequest_agents::orchestrator::ActionResult,
    pub tier_events: &'a [sidequest_game::AffinityTierUpEvent],
    pub narration_state_delta: sidequest_protocol::StateDelta,
}

/// Build narration, party status, inventory, and RAG messages.
///
/// Story 15-20: `narration_state_delta` is pre-built via `build_protocol_delta`
/// using game-crate delta computation instead of inline construction.
///
/// Returns the merged footnotes (narrator footnotes + affinity tier-up
/// synthesis events) so the caller can forward them to
/// `sync_back_to_shared_session` for observer broadcasts. The returned Vec
/// is the same set baked into the Narration message sent to the acting
/// player, guaranteeing parity between what the acting player sees and
/// what gets rebroadcast to observers.
pub(super) async fn build_response_messages(
    ctx: &mut DispatchContext<'_>,
    rctx: &ResponseContext<'_>,
    messages: &mut Vec<GameMessage>,
) -> Vec<sidequest_protocol::Footnote> {
    let clean_narration = rctx.clean_narration;
    let result = rctx.result;
    let tier_events = rctx.tier_events;
    let narration_state_delta = rctx.narration_state_delta.clone();
    // Merge narrator footnotes with affinity tier-up events
    let mut footnotes = result.footnotes.clone();
    for event in tier_events {
        let summary_text = format!(
            "{}'s {} affinity reached tier {} — {}",
            event.character_name,
            event.affinity_name,
            event.new_tier,
            if event.narration_hint.is_empty() {
                "a new level of mastery"
            } else {
                &event.narration_hint
            },
        );
        let summary = NonBlankString::new(&summary_text)
            .expect("tier-up footnote summary is constructed non-empty from fixed format");
        footnotes.push(sidequest_protocol::Footnote {
            marker: None,
            fact_id: None,
            summary,
            category: sidequest_protocol::FactCategory::Ability,
            is_new: true,
        });
    }
    // Snapshot the merged footnotes for the return value. The clone inside
    // the Narration message below is moved into `ctx.tx.send`, so we keep
    // a parallel copy to forward to `sync_back_to_shared_session`.
    let merged_footnotes = footnotes.clone();

    // Send narration to the acting player IMMEDIATELY via the direct mpsc
    // channel. This is the fast-path per ADR-063: the acting player sees
    // narration within a few ms while the ~100-500ms RAG/embed work and
    // the ~15s deferred continuity validation run after
    // `build_response_messages` returns.
    //
    // The Narration is NOT pushed into the returned `messages` Vec. The
    // caller's ws writer loop iterates that Vec and flushes every entry
    // through the same `ctx.tx` channel — if Narration were in the Vec,
    // the acting player would receive every turn's narration twice (the
    // 2026-04-11 regression introduced in d3896421's dispatch decomposition
    // refactor and observed mid-playtest by Keith).
    //
    // Observers still see the narration: `sync_back_to_shared_session`
    // builds its own Narration message from the explicit `clean_narration`
    // + `footnotes` parameters and fans it out via the session channel.
    let narration_text_nbs = nbs_ctx(clean_narration, "clean_narration text");
    let narration_msg = GameMessage::Narration {
        payload: NarrationPayload {
            text: narration_text_nbs,
            state_delta: Some(narration_state_delta),
            footnotes,
        },
        player_id: ctx.player_id.to_string(),
    };
    let _ = ctx.tx.send(narration_msg).await;
    tracing::info!("Narration sent to client — state cleanup continues async");

    // RAG pipeline: convert new footnotes to discovered facts
    if !result.footnotes.is_empty() {
        let fact_source = if result.classified_intent.as_deref() == Some("Backstory") {
            sidequest_game::known_fact::FactSource::Backstory
        } else {
            sidequest_game::known_fact::FactSource::Discovery
        };
        let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
            &result.footnotes,
            ctx.char_name,
            ctx.turn_manager.interaction(),
            fact_source,
        );
        if !discovered.is_empty() {
            tracing::info!(
                count = discovered.len(),
                character = %ctx.char_name,
                interaction = ctx.turn_manager.interaction(),
                "rag.footnotes_to_discovered_facts"
            );
            if let Some(ref mut cj) = ctx.character_json {
                if let Some(facts_arr) = cj.get_mut("known_facts").and_then(|v| v.as_array_mut()) {
                    for df in &discovered {
                        if let Ok(fact_val) = serde_json::to_value(&df.fact) {
                            facts_arr.push(fact_val);
                        }
                    }
                    tracing::info!(
                        new_facts = discovered.len(),
                        total_facts = facts_arr.len(),
                        "rag.discovered_facts_applied_to_character"
                    );
                }
            }

            let turn = ctx.turn_manager.interaction();
            for df in &discovered {
                let lore_cat: sidequest_game::lore::LoreCategory = df.fact.category.into();
                let mut meta = std::collections::HashMap::new();
                meta.insert("source".to_string(), format!("{:?}", df.fact.source));
                meta.insert("character".to_string(), df.character_name.clone());
                meta.insert(
                    "confidence".to_string(),
                    format!("{:?}", df.fact.confidence),
                );
                super::lore_sync::accumulate_and_persist_lore(
                    ctx,
                    &df.fact.content,
                    lore_cat,
                    turn,
                    meta,
                )
                .await;
            }

            WatcherEventBuilder::new("rag", WatcherEventType::SubsystemExerciseSummary)
                .field("event", "rag.footnotes_to_lore")
                .field("total_footnotes", result.footnotes.len())
                .field("new_facts", discovered.len())
                .field("character", ctx.char_name)
                .send();
        }
    }

    // NarrationEnd follows the same fast-path as Narration: sent directly
    // to the acting player via `ctx.tx`, NOT pushed into the `messages`
    // Vec. A duplicate NARRATION_END double-flushes the narration buffer
    // in the UI and double-fires the canType unlock — part of the same
    // 2026-04-11 regression as the duplicate NARRATION. Observers receive
    // NarrationEnd via `sync_back_to_shared_session`.
    let narration_end = GameMessage::NarrationEnd {
        payload: NarrationEndPayload { state_delta: None },
        player_id: ctx.player_id.to_string(),
    };
    let _ = ctx.tx.send(narration_end).await;

    // ScrapbookEntry (story 33-18) — bundle the turn's narration excerpt,
    // new world facts, and present NPCs into a single atomic message so the
    // Scrapbook widget (story 33-17) can render a gallery card from one
    // payload instead of stitching together Narration + Image + registry
    // observer state. Emitted AFTER NarrationEnd per AC: world_facts and
    // npc_registry must be settled before the entry ships.
    //
    // Image metadata (scene_title/scene_type/image_url) is left None —
    // renders arrive on an async broadcast channel (render_integration.rs)
    // and are not guaranteed complete at narration-end time. The client
    // merges a later GameMessage::Image by turn_id. Threading the latest
    // completed render subject through DispatchContext is a follow-up.
    let turn_id: u64 = ctx.turn_manager.interaction();
    let turn_id_u32 = u32::try_from(turn_id).unwrap_or(u32::MAX);
    // Snapshot the OCEAN fallback count before we hand the registry slice to
    // build_scrapbook_entry (the filter lives inside the pure function now,
    // story 33-18 refactor — see scrapbook::build_scrapbook_entry).
    let npcs_disposition_fallback_count = ctx
        .npc_registry
        .iter()
        .filter(|e| e.last_seen_turn == turn_id_u32)
        .filter(|e| e.ocean_summary.is_empty())
        .count();
    // Thread the latest completed render into the scrapbook entry when the
    // async broadcaster has already produced an IMAGE for this turn. The
    // shared slot lives on AppState, keyed by "genre:world"; see
    // `AppState::record_latest_render` (populated by the job-routing
    // broadcaster in `lib.rs`). When no render is ready at emission time we
    // leave the fields `None` and the client merges a later
    // `GameMessage::Image` by turn_id, as before.
    let session_key = format!("{}:{}", ctx.genre_slug, ctx.world_slug);
    let (scene_title, scene_type, image_url) =
        match ctx.state.take_latest_render(&session_key, turn_id) {
            Some(render) => (render.scene_title, render.scene_type, Some(render.url)),
            None => (None, None, None),
        };
    let image_populated_at_emission = image_url.is_some();
    let location_nbs = nbs_ctx(ctx.current_location.as_str(), "current_location");
    let scrapbook_payload = scrapbook::build_scrapbook_entry(
        turn_id,
        location_nbs,
        scene_title,
        scene_type,
        image_url,
        clean_narration,
        &merged_footnotes,
        ctx.npc_registry,
    );
    // Degraded-path visibility (per OTEL rule — the GM panel is the lie
    // detector): record when the narrator produced text without a sentence
    // terminator (the extractor returns the entire trimmed narration as the
    // excerpt in that case), and count how many NPCs fell back from
    // `ocean_summary` to `role` because their OCEAN summary was empty. A
    // non-zero fallback count surfaces an upstream OCEAN pipeline gap in
    // the GM panel without changing the observable message shape.
    let excerpt_fallback_full_narration =
        scrapbook_payload.narrative_excerpt.as_str() == clean_narration.trim();
    WatcherEventBuilder::new("scrapbook", WatcherEventType::SubsystemExerciseSummary)
        .field("event", "scrapbook.entry_emitted")
        .field("turn_id", turn_id)
        .field("world_facts_count", scrapbook_payload.world_facts.len())
        .field("npcs_count", scrapbook_payload.npcs_present.len())
        .field(
            "excerpt_chars",
            scrapbook_payload.narrative_excerpt.as_str().chars().count(),
        )
        .field(
            "excerpt_fallback_full_narration",
            excerpt_fallback_full_narration,
        )
        .field(
            "npcs_disposition_fallback_count",
            npcs_disposition_fallback_count,
        )
        .field("image_populated_at_emission", image_populated_at_emission)
        .send();
    messages.push(GameMessage::ScrapbookEntry {
        payload: scrapbook_payload.clone(),
        player_id: ctx.player_id.to_string(),
    });

    // Persist scrapbook entry to SQLite for session-resume replay.
    if !ctx.genre_slug.is_empty() && !ctx.world_slug.is_empty() {
        match ctx
            .state
            .persistence()
            .append_scrapbook_entry(
                ctx.genre_slug,
                ctx.world_slug,
                ctx.player_name_for_save,
                &scrapbook_payload,
            )
            .await
        {
            Ok(()) => {
                tracing::debug!(turn_id, "scrapbook.entry_persisted");
            }
            Err(e) => {
                tracing::warn!(error = %e, turn_id, "scrapbook.entry_persist_failed");
            }
        }
    }

    // Party status — now carries the per-member sheet and inventory facets
    // (CHARACTER_SHEET and INVENTORY message types were collapsed into
    // PARTY_STATUS by 51576be so observers get every teammate's full sheet
    // and we never race against null characterSheet on the client).
    {
        let char_class_str: String = ctx
            .character_json
            .as_ref()
            .and_then(|cj| cj.get("char_class"))
            .and_then(|c| c.as_str())
            .unwrap_or("Adventurer")
            .to_string();
        let char_class = nbs_ctx(&char_class_str, "party_member.class");

        // Pull the acting player's cached sheet off PlayerState (populated at
        // chargen completion in dispatch/connect.rs) and use the live
        // dispatch-scope inventory for the inventory facet.
        let acting_sheet = {
            let holder = ctx.shared_session_holder.lock().await;
            match *holder {
                Some(ref ss_arc) => {
                    let ss = ss_arc.lock().await;
                    ss.players
                        .get(ctx.player_id)
                        .and_then(|ps| ps.sheet.clone())
                }
                None => None,
            }
        };
        let acting_inventory = Some(crate::shared_session::inventory_payload_from(ctx.inventory));

        let mut party_members = vec![PartyMember {
            player_id: nbs_ctx(ctx.player_id, "party_member.player_id"),
            name: nbs_ctx(ctx.player_name_for_save, "party_member.name"),
            character_name: NonBlankString::new(ctx.char_name).ok(),
            current_hp: *ctx.edge,
            max_hp: *ctx.max_edge,
            statuses: vec![],
            class: char_class,
            level: *ctx.level,
            portrait_url: None,
            current_location: NonBlankString::new(ctx.current_location).ok(),
            sheet: acting_sheet,
            inventory: acting_inventory,
        }];
        // Observers come from the helper so their cached sheet+inventory ride along.
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == ctx.player_id {
                    continue;
                }
                party_members.push(crate::shared_session::party_member_from(pid, ps));
            }
        }
        messages.push(GameMessage::PartyStatus {
            payload: PartyStatusPayload {
                members: party_members,
            },
            player_id: ctx.player_id.to_string(),
        });
    }

    // MAP_UPDATE
    tracing::debug!(
        rooms_count = ctx.rooms.len(),
        discovered_rooms_count = ctx.snapshot.discovered_rooms.len(),
        discovered_rooms = ?ctx.snapshot.discovered_rooms,
        current_location = %ctx.snapshot.location,
        "map_update.debug — room graph state"
    );
    let explored_locs: Vec<sidequest_protocol::ExploredLocation> = if !ctx.rooms.is_empty() {
        let locs = sidequest_game::build_room_graph_explored(
            &ctx.rooms,
            &ctx.snapshot.discovered_rooms,
            &ctx.snapshot.location,
        );
        tracing::debug!(
            explored_count = locs.len(),
            room_exits_total = locs.iter().map(|l| l.room_exits.len()).sum::<usize>(),
            "map_update.debug — room graph explored result"
        );
        locs
    } else {
        ctx.discovered_regions
            .iter()
            .filter_map(|name| {
                NonBlankString::new(name).ok().map(|nbs| {
                    sidequest_protocol::ExploredLocation {
                        // Region mode has no separate slug — id mirrors name.
                        id: name.clone(),
                        name: nbs,
                        x: 0,
                        y: 0,
                        location_type: String::new(),
                        connections: vec![],
                        room_exits: vec![],
                        room_type: String::new(),
                        size: None,
                        is_current_room: name == ctx.current_location.as_str(),
                        tactical_grid: None,
                    }
                })
            })
            .collect()
    };
    super::emit_map_update_telemetry(
        "turn",
        ctx.player_id,
        ctx.current_location,
        &explored_locs,
        ctx.cartography_metadata.as_ref(),
    );
    let map_location = nbs_ctx(ctx.current_location, "map_update.current_location");
    messages.push(GameMessage::MapUpdate {
        payload: MapUpdatePayload {
            current_location: map_location.clone(),
            region: map_location,
            explored: explored_locs,
            fog_bounds: None,
            cartography: ctx.cartography_metadata.clone(),
        },
        player_id: ctx.player_id.to_string(),
    });

    // Confrontation overlay — uses the shared builder so the session-restore
    // path in `dispatch/connect.rs` emits an identical wire payload.
    if let Some(ref enc) = ctx.snapshot.encounter {
        let actors: Vec<sidequest_protocol::ConfrontationActor> = enc
            .actors
            .iter()
            .filter_map(|a| {
                let name = NonBlankString::new(&a.name).ok()?;
                let role = NonBlankString::new(&a.role).ok()?;
                let portrait = ctx
                    .npc_registry
                    .iter()
                    .find(|e| e.name.to_lowercase() == a.name.to_lowercase())
                    .and_then(|e| e.portrait_url.clone());
                Some(sidequest_protocol::ConfrontationActor {
                    name,
                    role,
                    portrait_url: portrait,
                })
            })
            .collect();
        let metric = &enc.metric;
        let direction_str = match metric.direction {
            sidequest_game::MetricDirection::Ascending => "ascending",
            sidequest_game::MetricDirection::Descending => "descending",
            sidequest_game::MetricDirection::Bidirectional => "bidirectional",
            _ => "ascending",
        };
        let def = crate::find_confrontation_def(&ctx.confrontation_defs, &enc.encounter_type);
        let label_text = def
            .map(|d| d.label.clone())
            .unwrap_or_else(|| enc.encounter_type.replace('_', " "));
        let category_text = def
            .map(|d| d.category.clone())
            .unwrap_or_else(|| enc.encounter_type.clone());
        let metric_name = NonBlankString::new(&metric.name)
            .expect("EncounterMetric.name is set non-empty at encounter construction (see StructuredEncounter constructors)");
        let confrontation_label = NonBlankString::new(&label_text)
            .expect("confrontation label is either genre-pack-defined (non-empty by YAML validation) or derived from encounter_type (non-empty by type dispatch)");
        let confrontation_category = NonBlankString::new(&category_text)
            .expect("confrontation category is either genre-pack-defined or equal to encounter_type (non-empty by type dispatch)");
        messages.push(GameMessage::Confrontation {
            payload: sidequest_protocol::ConfrontationPayload {
                encounter_type: enc.encounter_type.clone(),
                label: confrontation_label,
                category: confrontation_category,
                actors,
                metric: sidequest_protocol::ConfrontationMetric {
                    name: metric_name,
                    current: metric.current,
                    starting: metric.starting,
                    direction: direction_str.to_string(),
                    threshold_high: metric.threshold_high,
                    threshold_low: metric.threshold_low,
                },
                beats: def
                    .map(|d| {
                        d.beats
                            .iter()
                            .filter_map(|b| {
                                let id = NonBlankString::new(&b.id).ok()?;
                                let label = NonBlankString::new(&b.label).ok()?;
                                Some(sidequest_protocol::ConfrontationBeat {
                                    id,
                                    label,
                                    metric_delta: b.metric_delta,
                                    stat_check: b.stat_check.clone(),
                                    risk: b.risk.clone(),
                                    resolution: b.resolution.unwrap_or(false),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                secondary_stats: enc
                    .secondary_stats
                    .as_ref()
                    .and_then(|ss| serde_json::to_value(ss).ok()),
                genre_slug: ctx.genre_slug.to_string(),
                mood: enc.mood_override.clone().unwrap_or_default(),
                active: !enc.resolved,
            },
            player_id: ctx.player_id.to_string(),
        });
        if let Some(d) = def {
            if !d.beats.is_empty() {
                WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                    .field("action", "beats_sent")
                    .field("encounter_type", &enc.encounter_type)
                    .field("beat_count", d.beats.len())
                    .field(
                        "beat_ids",
                        d.beats.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
                    )
                    .send();
            }
        }
    }

    merged_footnotes
}

/// Build a `CONFRONTATION` message from a live `StructuredEncounter`.
///
/// Single source of truth for the confrontation wire payload. Callers:
/// - `build_response_messages` — normal turn response
/// - `dispatch_connect` — session restore (so a reloaded save with an active
///   encounter shows the overlay immediately, not after the first turn)
///
/// `confrontation_defs` supplies the full beat library / labels; if the
/// encounter type is unknown (defensive — should never happen for a saved
/// snapshot), the payload falls back to empty beats and a humanized label.
pub(super) fn build_confrontation_message(
    enc: &sidequest_game::encounter::StructuredEncounter,
    npc_registry: &[crate::NpcRegistryEntry],
    confrontation_defs: &[sidequest_genre::ConfrontationDef],
    genre_slug: &str,
    player_id: &str,
) -> GameMessage {
    let actors: Vec<sidequest_protocol::ConfrontationActor> = enc
        .actors
        .iter()
        .filter_map(|a| {
            let name = sidequest_protocol::NonBlankString::new(&a.name).ok()?;
            let role = sidequest_protocol::NonBlankString::new(&a.role).ok()?;
            let portrait = npc_registry
                .iter()
                .find(|e| e.name.to_lowercase() == a.name.to_lowercase())
                .and_then(|e| e.portrait_url.clone());
            Some(sidequest_protocol::ConfrontationActor {
                name,
                role,
                portrait_url: portrait,
            })
        })
        .collect();
    let metric = &enc.metric;
    let direction_str = match metric.direction {
        sidequest_game::MetricDirection::Ascending => "ascending",
        sidequest_game::MetricDirection::Descending => "descending",
        sidequest_game::MetricDirection::Bidirectional => "bidirectional",
        _ => "ascending",
    };
    let def = crate::find_confrontation_def(confrontation_defs, &enc.encounter_type);
    let label_text = def
        .map(|d| d.label.clone())
        .unwrap_or_else(|| enc.encounter_type.replace('_', " "));
    let category_text = def
        .map(|d| d.category.clone())
        .unwrap_or_else(|| enc.encounter_type.clone());
    let metric_name = sidequest_protocol::NonBlankString::new(&metric.name)
        .expect("EncounterMetric.name is set non-empty at encounter construction");
    let confrontation_label = sidequest_protocol::NonBlankString::new(&label_text)
        .expect("confrontation label is either genre-pack-defined or derived from encounter_type");
    let confrontation_category = sidequest_protocol::NonBlankString::new(&category_text)
        .expect("confrontation category is either genre-pack-defined or equal to encounter_type");
    GameMessage::Confrontation {
        payload: sidequest_protocol::ConfrontationPayload {
            encounter_type: enc.encounter_type.clone(),
            label: confrontation_label,
            category: confrontation_category,
            actors,
            metric: sidequest_protocol::ConfrontationMetric {
                name: metric_name,
                current: metric.current,
                starting: metric.starting,
                direction: direction_str.to_string(),
                threshold_high: metric.threshold_high,
                threshold_low: metric.threshold_low,
            },
            beats: def
                .map(|d| {
                    d.beats
                        .iter()
                        .filter_map(|b| {
                            let id = sidequest_protocol::NonBlankString::new(&b.id).ok()?;
                            let label = sidequest_protocol::NonBlankString::new(&b.label).ok()?;
                            Some(sidequest_protocol::ConfrontationBeat {
                                id,
                                label,
                                metric_delta: b.metric_delta,
                                stat_check: b.stat_check.clone(),
                                risk: b.risk.clone(),
                                resolution: b.resolution.unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            secondary_stats: enc
                .secondary_stats
                .as_ref()
                .and_then(|ss| serde_json::to_value(ss).ok()),
            genre_slug: genre_slug.to_string(),
            mood: enc.mood_override.clone().unwrap_or_default(),
            active: !enc.resolved,
        },
        player_id: player_id.to_string(),
    }
}
