//! Response message construction — narration, party status, inventory, map, encounters.

use sidequest_protocol::{
    GameMessage, InventoryPayload, MapUpdatePayload, NarrationEndPayload, NarrationPayload,
    PartyMember, PartyStatusPayload,
};

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Build narration, party status, inventory, and RAG messages.
///
/// Story 15-20: `narration_state_delta` is pre-built via `build_protocol_delta`
/// using game-crate delta computation instead of inline construction.
pub(super) async fn build_response_messages(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    _narration_text: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
    tier_events: &[sidequest_game::AffinityTierUpEvent],
    _effective_action: &str,
    messages: &mut Vec<GameMessage>,
    narration_state_delta: sidequest_protocol::StateDelta,
) {
    // Merge narrator footnotes with affinity tier-up events
    let mut footnotes = result.footnotes.clone();
    for event in tier_events {
        footnotes.push(sidequest_protocol::Footnote {
            marker: None,
            fact_id: None,
            summary: format!(
                "{}'s {} affinity reached tier {} — {}",
                event.character_name,
                event.affinity_name,
                event.new_tier,
                if event.narration_hint.is_empty() { "a new level of mastery" } else { &event.narration_hint },
            ),
            category: sidequest_protocol::FactCategory::Ability,
            is_new: true,
        });
    }

    // Send narration to client IMMEDIATELY
    let narration_msg = GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.to_string(),
            state_delta: Some(narration_state_delta),
            footnotes,
        },
        player_id: ctx.player_id.to_string(),
    };
    messages.push(narration_msg.clone());
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

            let turn = ctx.turn_manager.interaction() as u64;
            for df in &discovered {
                let lore_cat: sidequest_game::lore::LoreCategory = df.fact.category.into();
                let mut meta = std::collections::HashMap::new();
                meta.insert("source".to_string(), format!("{:?}", df.fact.source));
                meta.insert("character".to_string(), df.character_name.clone());
                meta.insert("confidence".to_string(), format!("{:?}", df.fact.confidence));
                super::lore_sync::accumulate_and_persist_lore(ctx, &df.fact.content, lore_cat, turn, meta).await;
            }

            WatcherEventBuilder::new("rag", WatcherEventType::SubsystemExerciseSummary)
                .field("event", "rag.footnotes_to_lore")
                .field("total_footnotes", result.footnotes.len())
                .field("new_facts", discovered.len())
                .field("character", ctx.char_name)
                .send();
        }
    }

    let narration_end = GameMessage::NarrationEnd {
        payload: NarrationEndPayload {
            state_delta: None,
        },
        player_id: ctx.player_id.to_string(),
    };
    messages.push(narration_end.clone());
    let _ = ctx.tx.send(narration_end).await;

    // Party status
    {
        let char_class: String = ctx
            .character_json
            .as_ref()
            .and_then(|cj| cj.get("char_class"))
            .and_then(|c| c.as_str())
            .unwrap_or("Adventurer")
            .to_string();

        let mut party_members = vec![PartyMember {
            player_id: ctx.player_id.to_string(),
            name: ctx.player_name_for_save.to_string(),
            character_name: ctx.char_name.to_string(),
            current_hp: *ctx.hp,
            max_hp: *ctx.max_hp,
            statuses: vec![],
            class: char_class,
            level: *ctx.level,
            portrait_url: None,
            current_location: ctx.current_location.clone(),
        }];
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == ctx.player_id {
                    continue;
                }
                party_members.push(PartyMember {
                    player_id: pid.clone(),
                    name: ps.player_name.clone(),
                    character_name: ps
                        .character_name
                        .clone()
                        .unwrap_or_else(|| ps.player_name.clone()),
                    current_hp: ps.character_hp,
                    max_hp: ps.character_max_hp,
                    statuses: vec![],
                    class: String::new(),
                    level: ps.character_level,
                    portrait_url: None,
                    current_location: ps.display_location.clone(),
                });
            }
        }
        messages.push(GameMessage::PartyStatus {
            payload: PartyStatusPayload {
                members: party_members,
            },
            player_id: ctx.player_id.to_string(),
        });
    }

    // Inventory
    messages.push(GameMessage::Inventory {
        payload: InventoryPayload {
            items: ctx
                .inventory
                .carried()
                .map(|item| sidequest_protocol::InventoryItem {
                    name: item.name.as_str().to_string(),
                    item_type: item.category.as_str().to_string(),
                    equipped: item.equipped,
                    quantity: item.quantity,
                    description: item.description.as_str().to_string(),
                })
                .collect(),
            gold: ctx.inventory.gold,
        },
        player_id: ctx.player_id.to_string(),
    });

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
            .map(|name| sidequest_protocol::ExploredLocation {
                name: name.clone(),
                x: 0,
                y: 0,
                location_type: String::new(),
                connections: vec![],
                room_exits: vec![],
                room_type: String::new(),
                size: None,
                is_current_room: name == ctx.current_location.as_str(),
                tactical_grid: None,
            })
            .collect()
    };
    messages.push(GameMessage::MapUpdate {
        payload: MapUpdatePayload {
            current_location: ctx.current_location.clone(),
            region: ctx.current_location.clone(),
            explored: explored_locs,
            fog_bounds: None,
            cartography: ctx.cartography_metadata.clone(),
        },
        player_id: ctx.player_id.to_string(),
    });

    // Confrontation overlay
    if let Some(ref enc) = ctx.snapshot.encounter {
        let actors: Vec<sidequest_protocol::ConfrontationActor> = enc.actors.iter().map(|a| {
            let portrait = ctx.npc_registry.iter()
                .find(|e| e.name.to_lowercase() == a.name.to_lowercase())
                .and_then(|e| e.portrait_url.clone());
            sidequest_protocol::ConfrontationActor {
                name: a.name.clone(),
                role: a.role.clone(),
                portrait_url: portrait,
            }
        }).collect();
        let metric = &enc.metric;
        let direction_str = match metric.direction {
            sidequest_game::MetricDirection::Ascending => "ascending",
            sidequest_game::MetricDirection::Descending => "descending",
            sidequest_game::MetricDirection::Bidirectional => "bidirectional",
            _ => "ascending",
        };
        let def = crate::find_confrontation_def(&ctx.confrontation_defs, &enc.encounter_type);
        messages.push(GameMessage::Confrontation {
            payload: sidequest_protocol::ConfrontationPayload {
                encounter_type: enc.encounter_type.clone(),
                label: def.map(|d| d.label.clone()).unwrap_or_else(|| enc.encounter_type.replace('_', " ")),
                category: def.map(|d| d.category.clone()).unwrap_or_else(|| enc.encounter_type.clone()),
                actors,
                metric: sidequest_protocol::ConfrontationMetric {
                    name: metric.name.clone(),
                    current: metric.current,
                    starting: metric.starting,
                    direction: direction_str.to_string(),
                    threshold_high: metric.threshold_high,
                    threshold_low: metric.threshold_low,
                },
                beats: def.map(|d| d.beats.iter().map(|b| sidequest_protocol::ConfrontationBeat {
                    id: b.id.clone(),
                    label: b.label.clone(),
                    metric_delta: b.metric_delta,
                    stat_check: b.stat_check.clone(),
                    risk: b.risk.clone(),
                    resolution: b.resolution.unwrap_or(false),
                }).collect()).unwrap_or_default(),
                secondary_stats: enc.secondary_stats.as_ref().and_then(|ss| serde_json::to_value(ss).ok()),
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
                    .field("beat_ids", d.beats.iter().map(|b| b.id.clone()).collect::<Vec<_>>())
                    .send();
            }
        }
    }
}
