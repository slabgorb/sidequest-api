//! Post-narration state mutations: quests, XP, affinity, items, resources.
//! CombatPatch/ChasePatch application deleted in story 28-9.
//! Beat selections via StructuredEncounter replace typed patches.

use sidequest_genre::GenreLoader;

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Result of applying state mutations.
pub(crate) struct MutationResult {
    pub tier_events: Vec<sidequest_game::AffinityTierUpEvent>,
}

/// Apply post-narration state mutations: quests, XP, affinity, items.
pub(crate) async fn apply_state_mutations(
    ctx: &mut DispatchContext<'_>,
    result: &sidequest_agents::orchestrator::ActionResult,
    clean_narration: &str,
    effective_action: &str,
) -> MutationResult {
    let mut all_tier_events = Vec::new();
    let gold_before = ctx.inventory.gold;

    // CombatPatch/ChasePatch blocks deleted in story 28-9.
    // Beat selections via StructuredEncounter replace typed patches.

    // Quest log updates — merge narrator-extracted quest changes
    if !result.quest_updates.is_empty() {
        for (quest_name, status) in &result.quest_updates {
            ctx.quest_log.insert(quest_name.clone(), status.clone());
            tracing::info!(quest = %quest_name, status = %status, "quest.updated");
        }
    }

    // XP award based on action type
    {
        let xp_award = if ctx.in_combat() {
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

                    let combined_lower = format!(
                        "{} {}",
                        effective_action.to_lowercase(),
                        clean_narration.to_lowercase(),
                    );
                    for aff_def in genre_affinities {
                        let matches_trigger = aff_def.triggers.iter().any(|trigger| {
                            trigger
                                .split_whitespace()
                                .map(|w| w.to_lowercase())
                                .filter(|w| w.len() >= 4)
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
                            .send();
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
        let lower = name_trimmed.to_lowercase();
        if lower.starts_with("the ") && word_count > 5 {
            tracing::warn!(item_name = %item_def.name, "Rejected item: sentence-like name");
            continue;
        }
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
                state: sidequest_game::ItemState::Carried,
            };
            let _ = ctx.add_item(item);
            tracing::info!(item_name = %item_def.name, "Item added to inventory from LLM extraction");
            WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                .field("action", "item_added")
                .field("item_name", &item_def.name)
                .field("category", &valid_cat)
                .field("inventory_size", ctx.inventory.items.len())
                .send();
        }
    }

    // Story 35-4: Treasure-as-XP — gold gained on the surface grants affinity progress.
    {
        let gold_delta = ctx.inventory.gold - gold_before;
        if gold_delta > 0 {
            let genre_code = sidequest_genre::GenreCode::new(ctx.genre_slug);
            if let Ok(code) = genre_code {
                let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
                if let Ok(pack) = loader.load(&code) {
                    let config = sidequest_game::TreasureXpConfig {
                        xp_affinity: pack.rules.xp_affinity.clone(),
                    };
                    let rooms = if ctx.rooms.is_empty() { None } else { Some(ctx.rooms.as_slice()) };
                    let txp_result = sidequest_game::apply_treasure_xp(
                        &mut ctx.snapshot,
                        gold_delta as u32,
                        &config,
                        rooms,
                    );
                    if txp_result.applied {
                        WatcherEventBuilder::new("treasure_xp", WatcherEventType::StateTransition)
                            .field("event", "treasure.extracted")
                            .field("gold_amount", txp_result.gold_amount)
                            .field("affinity_name", txp_result.affinity_name.as_deref().unwrap_or("unknown"))
                            .field("new_progress", txp_result.new_progress.unwrap_or(0))
                            .field("location", ctx.current_location.as_str())
                            .send();
                    }
                }
            }
        }
    }

    // Resource delta application (story 16-1 + epic 16 ResourcePool wiring)
    if !result.resource_deltas.is_empty() {
        let turn = ctx.turn_manager.interaction() as u64;
        for (name, delta) in &result.resource_deltas {
            let op = if *delta >= 0.0 {
                sidequest_game::ResourcePatchOp::Add
            } else {
                sidequest_game::ResourcePatchOp::Subtract
            };
            let value = delta.abs();
            match ctx.snapshot.process_resource_patch_with_lore(name, op, value, ctx.lore_store, turn) {
                Ok(patch_result) => {
                    // Phase 5: snapshot.resources is already mutated in-place
                    // by process_resource_patch_with_lore — no sync needed.
                    let mut builder = WatcherEventBuilder::new("resource_pool", WatcherEventType::StateTransition)
                        .field("event", "resource_pool.patched")
                        .field("resource", name)
                        .field("delta", delta)
                        .field("old_value", patch_result.old_value)
                        .field("new_value", patch_result.new_value)
                        .field("turn", turn);
                    // OTEL label/max come from the pool itself now (phase 3).
                    if let Some(pool) = ctx.snapshot.resources.get(name) {
                        builder = builder.field("max", pool.max).field("label", pool.label.clone());
                    }
                    if !patch_result.crossed_thresholds.is_empty() {
                        builder = builder.field("thresholds_crossed",
                            patch_result.crossed_thresholds.iter().map(|t| t.event_id.clone()).collect::<Vec<_>>());
                    }
                    builder.send();
                    tracing::info!(
                        resource = %name,
                        delta = %delta,
                        old = patch_result.old_value,
                        new = patch_result.new_value,
                        thresholds_crossed = patch_result.crossed_thresholds.len(),
                        "resource_pool.delta_applied"
                    );
                }
                Err(e) => {
                    // No silent fallback — if the narrator named a resource
                    // that doesn't exist in the pool, that's a configuration
                    // bug worth surfacing. The legacy resource_state fallback
                    // used to paper over this by silently mutating the loose
                    // HashMap; phase 1c removes that path per CLAUDE.md rules.
                    tracing::warn!(
                        resource = %name,
                        delta = %delta,
                        error = %e,
                        "resource_pool.delta_rejected — resource not declared in genre pack"
                    );
                    WatcherEventBuilder::new("resource_pool", WatcherEventType::ValidationWarning)
                        .field("event", "resource_pool.delta_rejected")
                        .field("resource", name)
                        .field("delta", delta)
                        .field("error", format!("{e}"))
                        .field("turn", turn)
                        .send();
                }
            }
        }
    }

    MutationResult {
        tier_events: all_tier_events,
    }
}
