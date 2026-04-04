//! Post-narration state mutations: combat HP, quests, XP, affinity, items, resources.

use sidequest_genre::GenreLoader;

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Result of applying state mutations — includes combat transition info for overlays.
pub(crate) struct MutationResult {
    pub tier_events: Vec<sidequest_game::AffinityTierUpEvent>,
    /// True if combat was active before mutations but is now inactive.
    pub combat_just_ended: bool,
    /// True if combat was inactive before mutations but is now active.
    pub combat_just_started: bool,
}

/// Apply post-narration state mutations: combat HP, quests, XP, affinity, items.
pub(crate) async fn apply_state_mutations(
    ctx: &mut DispatchContext<'_>,
    result: &sidequest_agents::orchestrator::ActionResult,
    clean_narration: &str,
    effective_action: &str,
) -> MutationResult {
    let mut all_tier_events = Vec::new();
    let combat_before = ctx.combat_state.in_combat();

    // Combat state — apply typed CombatPatch from creature_smith
    if let Some(ref combat_patch) = result.combat_patch {
        WatcherEventBuilder::new("combat", WatcherEventType::AgentSpanOpen)
            .field("action", "combat_patch_received")
            .field("in_combat", &combat_patch.in_combat)
            .field("hp_changes", &combat_patch.hp_changes)
            .field("turn_order", &combat_patch.turn_order)
            .field("current_turn", &combat_patch.current_turn)
            .field("drama_weight", &combat_patch.drama_weight)
            .field("advance_round", combat_patch.advance_round)
            .send(ctx.state);
        let was_in_combat = ctx.combat_state.in_combat();

        // Combat start → engage() with player + NPCs from the patch (not all known NPCs)
        if let Some(in_combat) = combat_patch.in_combat {
            if in_combat && !was_in_combat {
                // Build combatant list from the patch, not from npc_registry.
                // Prefer turn_order if provided; otherwise use hp_changes targets.
                let combatants = if combat_patch
                    .turn_order
                    .as_ref()
                    .map_or(false, |o| !o.is_empty())
                {
                    combat_patch.turn_order.clone().unwrap()
                } else {
                    let mut names: Vec<String> = vec![ctx.char_name.to_string()];
                    if let Some(ref hp_changes) = combat_patch.hp_changes {
                        for target in hp_changes.keys() {
                            if !names.iter().any(|n| n.eq_ignore_ascii_case(target)) {
                                names.push(target.clone());
                            }
                        }
                    }
                    names
                };
                ctx.combat_state.engage(combatants);
                tracing::info!(
                    turn_order = ?ctx.combat_state.turn_order(),
                    current_turn = ?ctx.combat_state.current_turn(),
                    "combat.engaged"
                );
                WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                    .field("action", "combat_started")
                    .field("turn_order", ctx.combat_state.turn_order())
                    .field("current_turn", ctx.combat_state.current_turn())
                    .field("combatant_count", ctx.combat_state.turn_order().len())
                    .send(ctx.state);

                // Turn mode transition: FreePlay → Structured
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode = old_mode
                        .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat start");
                    if ss.turn_mode.should_use_barrier() && ss.turn_barrier.is_none() {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss.players.keys().cloned(),
                            );
                        let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                        ss.turn_barrier =
                            Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session, adaptive,
                            ));
                    }
                }
            } else if !in_combat && was_in_combat {
                ctx.combat_state.disengage();
                tracing::info!("combat.disengaged");
                WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                    .field("action", "combat_ended")
                    .send(ctx.state);

                // Turn mode transition: Structured → FreePlay
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode =
                        old_mode.apply(sidequest_game::turn_mode::TurnModeTransition::CombatEnded);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat end");
                }
            }
        }

        // Apply HP deltas
        if let Some(ref hp_changes) = combat_patch.hp_changes {
            let char_name_lower = ctx.player_name_for_save.to_lowercase();
            for (target, delta) in hp_changes {
                let target_lower = target.to_lowercase();
                if target_lower == char_name_lower
                    || ctx
                        .character_json
                        .as_ref()
                        .and_then(|cj| cj.get("name"))
                        .and_then(|n| n.as_str())
                        .map(|n| n.to_lowercase() == target_lower)
                        .unwrap_or(false)
                {
                    let old_hp = *ctx.hp;
                    *ctx.hp = sidequest_game::clamp_hp(*ctx.hp, *delta, *ctx.max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = *ctx.hp, "combat.patch.hp_applied");
                    WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                        .field("action", "hp_change")
                        .field("target", target)
                        .field("target_type", "player")
                        .field("delta", delta)
                        .field("old_hp", old_hp)
                        .field("new_hp", *ctx.hp)
                        .field("max_hp", *ctx.max_hp)
                        .send(ctx.state);
                } else if let Some(npc) = ctx.npc_registry.iter_mut().find(|n| n.name.to_lowercase() == target_lower) {
                    // Initialize NPC max_hp on first damage if not yet set
                    if npc.max_hp == 0 {
                        // Estimate: if the LLM is dealing damage, assume NPC has some HP.
                        // Set max_hp to a reasonable default so clamp_hp works.
                        npc.max_hp = 20;
                        npc.hp = npc.max_hp;
                    }
                    let old_npc_hp = npc.hp;
                    npc.hp = sidequest_game::clamp_hp(npc.hp, *delta, npc.max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = npc.hp, max_hp = npc.max_hp, "combat.patch.npc_hp_applied");
                    WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                        .field("action", "hp_change")
                        .field("target", target)
                        .field("target_type", "npc")
                        .field("delta", delta)
                        .field("old_hp", old_npc_hp)
                        .field("new_hp", npc.hp)
                        .field("max_hp", npc.max_hp)
                        .send(ctx.state);
                }
            }
        }

        // Apply turn_order/current_turn updates (mid-combat changes)
        if ctx.combat_state.in_combat() {
            if let Some(ref order) = combat_patch.turn_order {
                if !order.is_empty() {
                    ctx.combat_state.set_turn_order(order.clone());
                }
            }
            if let Some(ref turn) = combat_patch.current_turn {
                ctx.combat_state.set_current_turn(turn.clone());
            }
        }

        if let Some(dw) = combat_patch.drama_weight {
            ctx.combat_state.set_drama_weight(dw);
        }

        // Advance turn (handles round wrap internally).
        // Always advance when in combat — turn alternation is a mechanical game
        // rule, not an LLM decision.  The creature_smith prompt asks for
        // advance_round but Claude conservatively returns false most of the
        // time, leaving NPCs permanently unable to act.
        if ctx.combat_state.in_combat() {
            ctx.combat_state.advance_turn();
            WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                .field("action", "turn_advanced")
                .field("round", ctx.combat_state.round())
                .field("current_turn", ctx.combat_state.current_turn())
                .field("turn_order", ctx.combat_state.turn_order())
                .send(ctx.state);

            // NPC turns — resolve attacks for each NPC until it's the player's turn again.
            // Without this, combat is an asskicking simulator: the player attacks every turn
            // and NPCs never mechanically fight back (Claude just improvises damage in narration).
            let max_npc_turns = ctx.combat_state.turn_order().len();
            for _ in 0..max_npc_turns {
                if !ctx.combat_state.in_combat() {
                    break;
                }
                let current = ctx.combat_state.current_turn().unwrap_or("").to_string();
                if current.eq_ignore_ascii_case(ctx.char_name) {
                    break; // Player's turn — stop, let them act
                }

                // This is an NPC's turn. Resolve their attack against the player.
                // Scope borrows carefully to avoid borrow checker conflicts.
                let round_result = {
                    let npc_opt = ctx.snapshot.npcs.iter().find(|n| {
                        sidequest_game::combatant::Combatant::name(*n).eq_ignore_ascii_case(&current)
                    });
                    let player_opt = ctx.snapshot.characters.first();
                    match (npc_opt, player_opt) {
                        (Some(npc), Some(player)) => {
                            Some(ctx.combat_state.resolve_attack(&current, npc, ctx.char_name, player))
                        }
                        _ => None,
                    }
                };

                if let Some(round_result) = round_result {
                    // Apply NPC damage to player HP (both ctx.hp and snapshot)
                    for event in &round_result.damage_events {
                        *ctx.hp -= event.damage;
                        if *ctx.hp < 0 {
                            *ctx.hp = 0;
                        }
                        if let Some(ch) = ctx.snapshot.characters.first_mut() {
                            ch.core.hp = *ctx.hp;
                        }
                        WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                            .field("action", "npc_attack")
                            .field("attacker", &event.attacker)
                            .field("target", &event.target)
                            .field("damage", event.damage)
                            .field("target_hp_after", *ctx.hp)
                            .field("round", event.round)
                            .send(ctx.state);
                        tracing::info!(
                            attacker = %event.attacker,
                            target = %event.target,
                            damage = event.damage,
                            hp_after = *ctx.hp,
                            "combat.npc_attack_resolved"
                        );
                    }

                    // Check player death after NPC attack
                    if *ctx.hp <= 0 {
                        WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                            .field("action", "player_death")
                            .field("cause", "combat_damage")
                            .field("final_hp", *ctx.hp)
                            .field("round", ctx.combat_state.round())
                            .send(ctx.state);
                        tracing::warn!(
                            hp = *ctx.hp,
                            round = ctx.combat_state.round(),
                            "combat.player_death — character has fallen"
                        );
                        ctx.combat_state.disengage();
                        ctx.snapshot.player_dead = true;
                        break;
                    }

                    // Check victory/defeat after NPC attack
                    let npc_combatants: Vec<&dyn sidequest_game::combatant::Combatant> = ctx.snapshot.npcs.iter()
                        .filter(|n| ctx.combat_state.turn_order().iter().any(|t| t.eq_ignore_ascii_case(sidequest_game::combatant::Combatant::name(*n))))
                        .map(|n| n as &dyn sidequest_game::combatant::Combatant)
                        .collect();
                    let player_combatants: Vec<&dyn sidequest_game::combatant::Combatant> = ctx.snapshot.characters.iter()
                        .map(|c| c as &dyn sidequest_game::combatant::Combatant)
                        .collect();
                    if let Some(outcome) = ctx.combat_state.check_victory(&player_combatants, &npc_combatants) {
                        WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                            .field("action", "combat_outcome")
                            .field("outcome", format!("{:?}", outcome))
                            .field("round", ctx.combat_state.round())
                            .send(ctx.state);
                        tracing::info!(outcome = ?outcome, "combat.outcome_reached");
                        ctx.combat_state.disengage();
                        break;
                    }
                }

                // Advance to next combatant
                ctx.combat_state.advance_turn();
            }
        }
    }

    // Chase state — apply typed ChasePatch from dialectician
    if let Some(ref chase_patch) = result.chase_patch {
        if let Some(in_chase) = chase_patch.in_chase {
            if in_chase && ctx.chase_state.is_none() {
                // Start chase
                let chase_type = match chase_patch.chase_type.as_deref() {
                    Some("stealth") => sidequest_game::ChaseType::Stealth,
                    Some("negotiation") => sidequest_game::ChaseType::Negotiation,
                    _ => sidequest_game::ChaseType::Footrace,
                };
                let cs = sidequest_game::ChaseState::new(chase_type, 0.5);
                *ctx.chase_state = Some(cs);
                tracing::info!(chase_type = ?chase_type, "chase.engaged");

                WatcherEventBuilder::new("chase", WatcherEventType::StateTransition)
                    .field("action", "chase_started")
                    .field("chase_type", format!("{:?}", chase_type))
                    .send(ctx.state);
            } else if !in_chase && ctx.chase_state.is_some() {
                // Resolve chase
                if let Some(ref cs) = ctx.chase_state {
                    tracing::info!(rounds = cs.round(), separation = cs.separation(), "chase.resolved");
                    WatcherEventBuilder::new("chase", WatcherEventType::StateTransition)
                        .field("action", "chase_resolved")
                        .field("rounds", cs.round())
                        .field("final_separation", cs.separation())
                        .send(ctx.state);
                }
                *ctx.chase_state = None;
            }
        }

        // Apply chase tick if chase is active
        if let Some(ref mut cs) = ctx.chase_state {
            if let Some(delta) = chase_patch.separation_delta {
                cs.set_separation(cs.separation() + delta);
            }
            if let Some(ref phase) = chase_patch.phase {
                cs.set_phase(phase.clone());
            }
            if let Some(ref event) = chase_patch.event {
                cs.set_event(event.clone());
            }
            if let Some(roll) = chase_patch.roll {
                cs.record_roll(roll);
            }

            tracing::info!(
                round = cs.round(),
                separation = cs.separation(),
                phase = ?cs.phase(),
                resolved = cs.is_resolved(),
                "chase.tick"
            );

            WatcherEventBuilder::new("chase", WatcherEventType::StateTransition)
                .field("action", "chase_tick")
                .field("round", cs.round())
                .field("separation", cs.separation())
                .field_opt("separation_delta", &chase_patch.separation_delta)
                .send(ctx.state);

            // Auto-resolve: mark as resolved but keep state alive for audio mood.
            // Chase state is only cleared when narrator explicitly sends in_chase: false.
            // This prevents mood flickering (Tension → Exploration) on the resolution turn.
            if cs.is_resolved() {
                tracing::info!("chase.auto_resolved — escape roll exceeded threshold, state retained for mood");

                WatcherEventBuilder::new("chase", WatcherEventType::StateTransition)
                    .field("action", "chase_auto_resolved")
                    .field("note", "state retained — awaiting narrator in_chase=false to clear")
                    .send(ctx.state);
            }
        }
    }

    // Quest log updates — merge narrator-extracted quest changes
    if !result.quest_updates.is_empty() {
        for (quest_name, status) in &result.quest_updates {
            ctx.quest_log.insert(quest_name.clone(), status.clone());
            tracing::info!(quest = %quest_name, status = %status, "quest.updated");
        }
    }

    // Bug 3: XP award based on action type
    {
        let xp_award = if ctx.combat_state.in_combat() {
            25 // combat actions give more XP
        } else {
            10 // exploration/dialogue gives base XP
        };
        *ctx.xp += xp_award;
        tracing::info!(
            xp_award = xp_award,
            total_xp = *ctx.xp,
            ctx.level = *ctx.level,
            "XP awarded"
        );

        // Check for level up
        let threshold = sidequest_game::xp_for_level(*ctx.level + 1);
        if *ctx.xp >= threshold {
            *ctx.level += 1;
            let new_max_hp = sidequest_game::level_to_hp(10, *ctx.level);
            let hp_gain = new_max_hp - *ctx.max_hp;
            *ctx.max_hp = new_max_hp;
            *ctx.hp = sidequest_game::clamp_hp(*ctx.hp + hp_gain, 0, *ctx.max_hp);
            tracing::info!(
                new_level = *ctx.level,
                new_max_hp = *ctx.max_hp,
                hp_gain = hp_gain,
                "Level up!"
            );
        }
    }

    // Affinity progression (Story F8) — check thresholds after XP/level-up.
    // Loads genre pack affinities via state to avoid adding another parameter.
    if let Some(ref cj) = ctx.character_json {
        if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            // Sync mutable fields
            ch.core.hp = *ctx.hp;
            ch.core.max_hp = *ctx.max_hp;
            ch.core.level = *ctx.level;
            ch.core.inventory = ctx.inventory.clone();

            // Increment affinity progress for any matching action triggers.
            let genre_code = sidequest_genre::GenreCode::new(ctx.genre_slug);
            if let Ok(code) = genre_code {
                let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
                if let Ok(pack) = loader.load(&code) {
                    let genre_affinities = &pack.progression.affinities;

                    // Increment progress for affinities whose triggers match the
                    // action OR narration.  Triggers are semantic sentences (e.g.
                    // "breaching corporate ICE") — match if ANY content word from
                    // the trigger appears in the combined text.  Old code did
                    // substring match of the full trigger sentence against the
                    // player's short action text, which never matched.
                    let combined_lower = format!(
                        "{} {}",
                        effective_action.to_lowercase(),
                        clean_narration.to_lowercase(),
                    );
                    for aff_def in genre_affinities {
                        let matches_trigger = aff_def.triggers.iter().any(|trigger| {
                            // Extract content words (3+ chars, skip stop words)
                            trigger
                                .split_whitespace()
                                .map(|w| w.to_lowercase())
                                .filter(|w| w.len() >= 4) // skip "a", "the", "in", "for"
                                .any(|word| combined_lower.contains(&word))
                        });
                        if matches_trigger {
                            sidequest_game::increment_affinity_progress(
                                &mut ch.affinities,
                                &aff_def.name,
                                1,
                            );
                            tracing::info!(
                                affinity = %aff_def.name,
                                progress = ch.affinities.iter().find(|a| a.name == aff_def.name).map(|a| a.progress).unwrap_or(0),
                                "Affinity progress incremented"
                            );
                        }
                    }

                    // Check thresholds for tier-ups
                    let thresholds_for = |name: &str| -> Option<Vec<u32>> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .map(|a| a.tier_thresholds.clone())
                    };
                    let narration_hint_for = |name: &str, tier: u8| -> Option<String> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .and_then(|a| {
                                a.unlocks.as_ref().and_then(|u| {
                                    let tier_data = match tier {
                                        1 => u.tier_1.as_ref(),
                                        2 => u.tier_2.as_ref(),
                                        3 => u.tier_3.as_ref(),
                                        _ => None,
                                    };
                                    tier_data.map(|t| t.description.clone())
                                })
                            })
                    };

                    let tier_events = sidequest_game::check_affinity_thresholds(
                        &mut ch.affinities,
                        ctx.char_name,
                        &thresholds_for,
                        &narration_hint_for,
                    );

                    for event in &tier_events {
                        tracing::info!(
                            affinity = %event.affinity_name,
                            old_tier = event.old_tier,
                            new_tier = event.new_tier,
                            character = %event.character_name,
                            "Affinity tier up!"
                        );
                    }
                    all_tier_events.extend(tier_events);
                }
            } // if let Ok(code)

            // Write updated character back to character_json
            if let Ok(updated_json) = serde_json::to_value(&ch) {
                *ctx.character_json = Some(updated_json);
            }
        }
    }

    // Merchant transactions — apply buy/sell extracted from narrator JSON block (story 15-16).
    if !result.merchant_transactions.is_empty() {
        let requests: Vec<sidequest_game::MerchantTransactionRequest> = result
            .merchant_transactions
            .iter()
            .filter_map(|tx| {
                let transaction_type = match tx.transaction_type.to_lowercase().as_str() {
                    "buy" => sidequest_game::TransactionType::Buy,
                    "sell" => sidequest_game::TransactionType::Sell,
                    other => {
                        tracing::warn!(tx_type = %other, "merchant.invalid_transaction_type");
                        return None;
                    }
                };
                Some(sidequest_game::MerchantTransactionRequest {
                    transaction_type,
                    item_id: tx.item_id.clone(),
                    merchant_name: tx.merchant.clone(),
                })
            })
            .collect();

        if !requests.is_empty() {
            let results = ctx.snapshot.apply_merchant_transactions(&requests);
            for (i, tx_result) in results.iter().enumerate() {
                match tx_result {
                    Ok(tx) => {
                        // Sync inventory back to ctx for downstream consumers
                        if let Some(ch) = ctx.snapshot.characters.first() {
                            *ctx.inventory = ch.core.inventory.clone();
                        }
                        WatcherEventBuilder::new("merchant", WatcherEventType::StateTransition)
                            .field("event", "merchant.transaction")
                            .field("type", format!("{:?}", tx.transaction_type))
                            .field("item", &tx.item_name)
                            .field("price", tx.price)
                            .field("merchant", &requests[i].merchant_name)
                            .send(ctx.state);
                        tracing::info!(
                            item = %tx.item_name,
                            price = tx.price,
                            tx_type = ?tx.transaction_type,
                            "merchant.transaction_applied"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "merchant.transaction_failed");
                    }
                }
            }
        }
    }

    // Item acquisition — driven by structured extraction from the LLM response.
    // The narrator emits items_gained in its JSON block when the player
    // actually acquires something.
    const VALID_ITEM_CATEGORIES: &[&str] = &[
        "weapon",
        "armor",
        "tool",
        "consumable",
        "quest",
        "treasure",
        "misc",
    ];
    for item_def in &result.items_gained {
        // Reject prose fragments: item names should be short noun phrases,
        // not sentences or long descriptions.
        let name_trimmed = item_def.name.trim();
        let word_count = name_trimmed.split_whitespace().count();
        if name_trimmed.len() > 60 || word_count > 8 {
            tracing::warn!(
                item_name = %item_def.name,
                len = name_trimmed.len(),
                words = word_count,
                "Rejected item: name too long (likely prose fragment)"
            );
            continue;
        }
        // Reject names that look like sentences (contain common verbs)
        let lower = name_trimmed.to_lowercase();
        if lower.starts_with("the ") && word_count > 5 {
            tracing::warn!(item_name = %item_def.name, "Rejected item: sentence-like name");
            continue;
        }
        // Validate category
        let category = item_def.category.trim().to_lowercase();
        let valid_cat = if VALID_ITEM_CATEGORIES.contains(&category.as_str()) {
            category
        } else {
            "misc".to_string()
        };
        let item_id = name_trimmed
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        if ctx.inventory.find(&item_id).is_some() {
            continue;
        }
        if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
            sidequest_protocol::NonBlankString::new(&item_id),
            sidequest_protocol::NonBlankString::new(name_trimmed),
            sidequest_protocol::NonBlankString::new(&item_def.description),
            sidequest_protocol::NonBlankString::new(&valid_cat),
            sidequest_protocol::NonBlankString::new("common"),
        ) {
            let item = sidequest_game::Item {
                id,
                name,
                description: desc,
                category: cat,
                value: 0,
                weight: 1.0,
                rarity,
                narrative_weight: 0.3,
                tags: vec![],
                equipped: false,
                quantity: 1,
                uses_remaining: None,
            };
            let _ = ctx.inventory.add(item, 50);
            tracing::info!(item_name = %item_def.name, "Item added to inventory from LLM extraction");
            WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                .field("action", "item_added")
                .field("item_name", &item_def.name)
                .field("category", &valid_cat)
                .field("inventory_size", ctx.inventory.items.len())
                .send(ctx.state);
        }
    }

    // Resource delta application (story 16-1)
    if !result.resource_deltas.is_empty() {
        for (name, delta) in &result.resource_deltas {
            if let Some(current) = ctx.resource_state.get_mut(name) {
                *current += delta;
                // Clamp to bounds if declaration exists
                if let Some(decl) = ctx.resource_declarations.iter().find(|d| d.name == *name) {
                    *current = current.clamp(decl.min, decl.max);
                }
                tracing::info!(resource = %name, delta = %delta, new_value = %current, "resource.delta_applied");
                {
                    let decl = ctx.resource_declarations.iter().find(|d| d.name == *name);
                    let mut builder = WatcherEventBuilder::new("resource", WatcherEventType::StateTransition)
                        .field("resource", name)
                        .field("delta", delta)
                        .field("new_value", *current);
                    if let Some(decl) = decl {
                        builder = builder.field("max", decl.max).field("label", &decl.label);
                    }
                    builder.send(ctx.state);
                }
            } else {
                tracing::debug!(resource = %name, "resource.delta_ignored — resource not in state");
            }
        }
    }

    let combat_after = ctx.combat_state.in_combat();
    MutationResult {
        tier_events: all_tier_events,
        combat_just_ended: combat_before && !combat_after,
        combat_just_started: !combat_before && combat_after,
    }
}
