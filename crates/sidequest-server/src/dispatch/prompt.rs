//! Narrator prompt context builder — assembles state summary for the LLM.

use sidequest_game::PreprocessedAction;

use crate::npc_context::build_npc_registry_context_budgeted;
use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Build the budgeted state_summary string for the narrator prompt.
/// Sections are gated by relevance flags from the Haiku preprocessor.
#[tracing::instrument(name = "turn.build_prompt_context", skip_all)]
pub(crate) async fn build_prompt_context(
    ctx: &mut DispatchContext<'_>,
    relevance: &PreprocessedAction,
) -> String {
    let turn_number = ctx.turn_manager.interaction() as u32;
    // Seed starter tropes if none are active yet (first turn)
    if ctx.trope_states.is_empty() && !ctx.trope_defs.is_empty() {
        // Prefer tropes with passive_progression so tick() can advance them.
        // Fall back to any trope if none have passive_progression.
        let mut seedable: Vec<&sidequest_genre::TropeDefinition> = ctx
            .trope_defs
            .iter()
            .filter(|d| d.passive_progression.is_some() && d.id.is_some())
            .collect();
        if seedable.is_empty() {
            seedable = ctx.trope_defs.iter().filter(|d| d.id.is_some()).collect();
        }
        let seed_count = seedable.len().min(3);
        tracing::info!(
            total_defs = ctx.trope_defs.len(),
            with_progression = ctx
                .trope_defs
                .iter()
                .filter(|d| d.passive_progression.is_some())
                .count(),
            seedable = seedable.len(),
            seed_count = seed_count,
            "Trope seeding — selecting starter tropes"
        );
        for def in &seedable[..seed_count] {
            if let Some(id) = &def.id {
                sidequest_game::trope::TropeEngine::activate(ctx.trope_states, id);
                tracing::info!(
                    trope_id = %id,
                    name = %def.name,
                    has_progression = def.passive_progression.is_some(),
                    "Seeded starter trope"
                );
                WatcherEventBuilder::new("trope", WatcherEventType::StateTransition)
                    .field("event", "trope_activated")
                    .field("trope_id", id)
                    .send(ctx.state);
            }
        }
    }

    // Build active trope context for the narrator prompt
    let trope_context = if ctx.trope_states.is_empty() {
        String::new()
    } else {
        let mut lines = vec!["Active narrative arcs:".to_string()];
        for ts in ctx.trope_states.iter() {
            if let Some(def) = ctx
                .trope_defs
                .iter()
                .find(|d| d.id.as_deref() == Some(ts.trope_definition_id()))
            {
                lines.push(format!(
                    "- {} ({}% progressed): {}",
                    def.name,
                    (ts.progression() * 100.0) as u32,
                    def.description
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>(),
                ));
                // Include the next unfired escalation beat as a hint
                for beat in &def.escalation {
                    if beat.at > ts.progression() {
                        lines.push(format!(
                            "  → Next beat at {}%: {}",
                            (beat.at * 100.0) as u32,
                            beat.event.chars().take(80).collect::<String>()
                        ));
                        break;
                    }
                }
            }
        }
        lines.join("\n")
    };

    // Build state summary for grounding narration (Bug 1: include location + entities)
    let mut state_summary = format!(
        "Character: {} (HP {}/{}, Level {}, XP {})\nGenre: {}",
        ctx.char_name, *ctx.hp, *ctx.max_hp, *ctx.level, *ctx.xp, ctx.genre_slug,
    );

    // Inject party roster so the narrator knows which characters are player-controlled
    // and never puppets them (gives them dialogue, actions, or internal state).
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            let other_pcs: Vec<String> = ss
                .players
                .iter()
                .filter(|(pid, _)| pid.as_str() != ctx.player_id)
                .filter_map(|(_, ps)| ps.character_name.clone())
                .collect();
            let co_located_names: Vec<String> = ss
                .co_located_players(ctx.player_id)
                .iter()
                .filter_map(|pid| {
                    ss.players
                        .get(pid.as_str())
                        .and_then(|ps| ps.character_name.clone())
                })
                .collect();

            if !other_pcs.is_empty() {
                state_summary.push_str(&format!(
                    "\n\nParty: {}.",
                    other_pcs.join(", ")
                ));
                if !co_located_names.is_empty() {
                    state_summary.push_str(&format!(
                        " Co-located: {}.",
                        co_located_names.join(", ")
                    ));
                }
                if turn_number <= 3 {
                    // Full rules for early turns
                    state_summary.push_str(concat!(
                        "\n\nPLAYER AGENCY — ABSOLUTE RULE:\n",
                        "Do NOT write dialogue, actions, thoughts, or internal state for ANY player character.\n",
                        "Players control their OWN characters. You control the WORLD, NPCs, and narration only.\n",
                        "PERSPECTIVE: Third-person omniscient. All characters named explicitly. Never use 'you'.",
                    ));
                } else {
                    // Compressed reminder after turn 3
                    state_summary.push_str(
                        " PLAYER AGENCY: Do not write dialogue/actions/thoughts for player characters. Third-person only."
                    );
                }
            }
        }
    }

    // Location constraint — prevent narrator from teleporting between scenes
    if !ctx.current_location.is_empty() {
        // Dialogue context: if the player interacted with an NPC in the last 2 turns,
        // any location mention in the action is likely dialogue (describing a place to
        // the NPC), not a travel intent. Strengthen the stay-put constraint.
        let turn_approx = ctx.turn_manager.interaction() as u32;
        let recent_npc_interaction = ctx
            .npc_registry
            .iter()
            .any(|e| turn_approx.saturating_sub(e.last_seen_turn) <= 2);
        let extra_dialogue_guard = if recent_npc_interaction {
            " IMPORTANT: The player is currently in dialogue with an NPC. If the player's \
             ctx.action mentions a location or place name, they are TALKING ABOUT that place, \
             NOT traveling there. Keep the scene at the current location. Only move if the \
             player explicitly ends the conversation and states they are leaving."
        } else {
            ""
        };
        state_summary.push_str(&format!(
            "\n\nLOCATION CONSTRAINT — THIS IS A HARD RULE:\nThe player is at: {}\nYou MUST continue the scene at this location. Do NOT introduce a new setting, move to a different area, or describe the player arriving somewhere else UNLESS the player explicitly says they want to travel or leave. If the player's action implies staying here, describe what happens HERE. Only change location when the player takes a deliberate travel action (e.g., 'I go to...', 'I leave...', 'I head north').{}",
            ctx.current_location, extra_dialogue_guard
        ));
    }

    // Inventory — full if player references items, compact summary otherwise
    if !ctx.inventory.items.is_empty() {
        if relevance.references_inventory {
            // Full inventory with descriptions and rules
            state_summary.push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
            state_summary.push_str("\nThe player currently possesses EXACTLY these items:");
            for item in &ctx.inventory.items {
                let equipped_tag = if item.equipped { " [EQUIPPED]" } else { "" };
                let qty_tag = if item.quantity > 1 {
                    format!(" (x{})", item.quantity)
                } else {
                    String::new()
                };
                state_summary.push_str(&format!(
                    "\n- {}{}{} — {} ({})",
                    item.name, equipped_tag, qty_tag, item.description, item.category
                ));
            }
            state_summary.push_str(&format!("\nGold: {}", ctx.inventory.gold));
            state_summary.push_str(concat!(
                "\n\nINVENTORY RULES (HARD CONSTRAINTS — violations break the game):",
                "\n1. If the player uses an item on this list, it WORKS. The item is real and present.",
                "\n2. If the player uses an item NOT on this list, it FAILS — they don't have it.",
                "\n3. NEVER narrate an item being lost, stolen, broken, or missing unless the game",
                "\n   engine explicitly removes it. The inventory list above is the TRUTH.",
                "\n4. [EQUIPPED] items are currently in hand/worn — the player does not need to 'find'",
                "\n   or 'reach for' them. They are ready to use immediately.",
            ));
        } else {
            // Compact: equipped items + count only
            let equipped: Vec<String> = ctx.inventory.items.iter()
                .filter(|i| i.equipped)
                .map(|i| i.name.to_string())
                .collect();
            let equipped_str = if equipped.is_empty() {
                "none equipped".to_string()
            } else {
                equipped.join(", ")
            };
            state_summary.push_str(&format!(
                "\n\nInventory: {} items ({}), {} gold.",
                ctx.inventory.items.len(), equipped_str, ctx.inventory.gold
            ));
        }
    } else {
        state_summary.push_str("\n\nThe player has NO items.");
    }

    // Quest log — inject active quests so narrator can reference them
    if !ctx.quest_log.is_empty() {
        state_summary.push_str("\n\nACTIVE QUESTS:\n");
        for (quest_name, status) in ctx.quest_log.iter() {
            state_summary.push_str(&format!("- {}: {}\n", quest_name, status));
        }
        state_summary.push_str("Reference active quests when narratively relevant. Update quest status in quest_updates when objectives change.\n");
    }

    // Resource state injection (story 16-1)
    if !ctx.resource_declarations.is_empty() {
        state_summary.push_str("\n\nGENRE RESOURCES — Current State:\n");
        for decl in ctx.resource_declarations {
            let current = ctx
                .resource_state
                .get(&decl.name)
                .copied()
                .unwrap_or(decl.starting);
            let vol_label = if decl.voluntary {
                "voluntary"
            } else {
                "involuntary"
            };
            let mut line = format!("{}: {}/{} ({})", decl.label, current, decl.max, vol_label);
            if decl.decay_per_turn.abs() > f64::EPSILON {
                line.push_str(&format!(", decay {}/turn", decl.decay_per_turn.abs()));
            }
            state_summary.push_str(&format!("- {}\n", line));
        }
        state_summary.push_str("When narrative events affect these resources, include resource_deltas in your JSON block.\n");
    }

    // Structured encounter context — covers both combat and chase via StructuredEncounter
    {
        let encounter = if ctx.combat_state.in_combat() {
            Some(sidequest_game::StructuredEncounter::from_combat_state(ctx.combat_state))
        } else {
            ctx.chase_state.as_ref().map(sidequest_game::StructuredEncounter::from_chase_state)
        };
        if let Some(ref enc) = encounter {
            WatcherEventBuilder::new("encounter", WatcherEventType::AgentSpanOpen)
                .field("action", "prompt_injection")
                .field("encounter_type", &enc.encounter_type)
                .field("beat", enc.beat)
                .field("metric", format!("{}: {}", enc.metric.name, enc.metric.current))
                .field("hint_count", enc.narrator_hints.len())
                .send(ctx.state);
            state_summary.push_str(&format!(
                "\n\nACTIVE ENCOUNTER ({}): beat {} | {}: {}/{}",
                enc.encounter_type,
                enc.beat,
                enc.metric.name,
                enc.metric.current,
                enc.metric.threshold_high.or(enc.metric.threshold_low).unwrap_or(0),
            ));
            if let Some(phase) = enc.structured_phase {
                state_summary.push_str(&format!(" | phase: {:?}", phase));
            }
            if !enc.actors.is_empty() {
                let actor_list: Vec<String> = enc.actors.iter()
                    .map(|a| format!("{} ({})", a.name, a.role))
                    .collect();
                state_summary.push_str(&format!("\nParticipants: {}", actor_list.join(", ")));
            }
            if !enc.narrator_hints.is_empty() {
                state_summary.push_str("\nEncounter context:");
                for hint in &enc.narrator_hints {
                    state_summary.push_str(&format!("\n- {}", hint));
                }
            }
        }
    }

    // Character identity — always included (compact)
    // Explicit PLAYER CHARACTER header prevents narrator from confusing PC/NPC attributes.
    state_summary.push_str("\n\n=== PLAYER CHARACTER ===\n");
    if let Some(ref cj) = ctx.character_json {
        if let Some(class) = cj.get("char_class").and_then(|c| c.as_str()) {
            state_summary.push_str(&format!("\nClass: {}", class));
        }
        if let Some(race) = cj.get("race").and_then(|r| r.as_str()) {
            state_summary.push_str(&format!("\nRace/Origin: {}", race));
        }
        if let Some(pronouns) = cj.get("pronouns").and_then(|p| p.as_str()) {
            if !pronouns.is_empty() {
                state_summary.push_str(&format!(
                    "\nPronouns: {} — ALWAYS use these pronouns for this character.",
                    pronouns
                ));
            }
        }
        if let Some(backstory) = cj.get("backstory").and_then(|b| b.as_str()) {
            state_summary.push_str(&format!("\nBackstory: {}", backstory));
        }

        // Abilities — full with rules if player references them, name-only list otherwise
        if let Some(hooks) = cj.get("hooks").and_then(|h| h.as_array()) {
            let hook_strs: Vec<&str> = hooks.iter().filter_map(|v| v.as_str()).collect();
            if !hook_strs.is_empty() {
                if relevance.references_ability {
                    state_summary.push_str("\n\nABILITY CONSTRAINTS — THIS IS A HARD RULE:\n");
                    state_summary.push_str("The PLAYER CHARACTER can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt. Do NOT apply these abilities to NPCs.\n");
                    state_summary.push_str("Allowed abilities:\n");
                    for h in &hook_strs {
                        state_summary.push_str(&format!("- {}\n", h));
                    }
                    state_summary.push_str("PROACTIVE MUTATION NARRATION: When the scene naturally creates an opportunity for the character's abilities/mutations to be relevant, weave them into the narration subtly.\n");
                } else {
                    state_summary.push_str(&format!(
                        "\nAbilities: {}.",
                        hook_strs.join(", ")
                    ));
                }
            }
        }
    }

    state_summary.push_str("\n=== END PLAYER CHARACTER ===\n");

    // Opening directive — turn 0 only, high-attention position before world context
    if turn_number == 0 {
        if let Some(ref directive) = ctx.opening_directive {
            state_summary.push_str("\n\n");
            state_summary.push_str(directive);
        }
    }

    // World context — full for first 5 turns (establishing setting), compressed after
    if !ctx.world_context.is_empty() {
        state_summary.push('\n');
        if turn_number <= 5 || ctx.world_context.len() < 400 {
            state_summary.push_str(ctx.world_context);
        } else {
            let hook: String = ctx.world_context
                .split(". ")
                .take(2)
                .collect::<Vec<_>>()
                .join(". ");
            state_summary.push_str(&hook);
            if !hook.ends_with('.') {
                state_summary.push('.');
            }
        }
    }

    // Inject known locations so the narrator uses canonical place names
    if !ctx.discovered_regions.is_empty() {
        state_summary.push_str("\n\nKNOWN LOCATIONS IN THIS WORLD:\n");
        state_summary.push_str("Use ONLY these location names when referring to places the party has visited or heard about. Do NOT invent new settlement names.\n");
        for region in ctx.discovered_regions.iter() {
            state_summary.push_str(&format!("- {}\n", region));
        }
    }
    // Also inject cartography region names from the shared session (if available)
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if !ss.region_names.is_empty() {
                if ctx.discovered_regions.is_empty() {
                    state_summary.push_str("\n\nWORLD LOCATIONS (from cartography):\n");
                    state_summary
                        .push_str("Use these canonical location names. Do NOT invent new ones.\n");
                } else {
                    state_summary.push_str("Additional world locations (not yet visited):\n");
                }
                for (region_id, _display_name) in &ss.region_names {
                    if !ctx
                        .discovered_regions
                        .iter()
                        .any(|r| r.to_lowercase() == *region_id)
                    {
                        state_summary.push_str(&format!("- {}\n", region_id));
                    }
                }
            }
        }
    }

    // Room graph navigation — inject current room + available exits
    if !ctx.rooms.is_empty() {
        if let Some(current_room) = ctx.rooms.iter().find(|r| r.id == *ctx.current_location || r.name == *ctx.current_location) {
            state_summary.push_str("\n\nROOM NAVIGATION (room-graph mode):\n");
            state_summary.push_str(&format!("Current room: {} — {}\n", current_room.name, current_room.description));
            if !current_room.exits.is_empty() {
                state_summary.push_str("Exits:\n");
                for exit in &current_room.exits {
                    state_summary.push_str(&format!("- {} → {} ({})\n", exit.direction, exit.target, exit.description));
                }
            }
            state_summary.push_str("When the player moves through an exit, update the location header to the target room name.\n");

            WatcherEventBuilder::new("navigation", WatcherEventType::StateTransition)
                .field("mode", "room_graph")
                .field("current_room", &current_room.id)
                .field("exit_count", current_room.exits.len())
                .send(ctx.state);
        }
    }

    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Inject tone context from narrative axes (story F2/F10)
    if let Some(ref ac) = ctx.axes_config {
        let tone_text = sidequest_game::format_tone_context(ac, ctx.axis_values);
        if !tone_text.is_empty() {
            state_summary.push_str(&tone_text);
        }
    }

    // Narration history — last 2 full, turns 3-5 first-sentence summary, 6+ dropped
    if !ctx.narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT HISTORY (most recent last):\n");
        let len = ctx.narration_history.len();
        let window_start = len.saturating_sub(5);
        for (i, entry) in ctx.narration_history[window_start..].iter().enumerate() {
            let from_end = len - window_start - i;
            if from_end <= 2 {
                // Last 2 turns: full text
                state_summary.push_str(entry);
                state_summary.push('\n');
            } else {
                // Older turns: first sentence only
                let first_sentence = entry.split(". ").next().unwrap_or(entry);
                let trimmed: String = first_sentence.chars().take(120).collect();
                state_summary.push_str(&format!("[...] {}\n", trimmed));
            }
        }
    }

    // NPC registry — full profiles if player references NPCs, compact otherwise
    let npc_context = build_npc_registry_context_budgeted(ctx.npc_registry, turn_number, relevance.references_npc);
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        // Prioritize lore categories based on current game state
        let priority_cats: Vec<sidequest_game::LoreCategory> = if ctx.combat_state.in_combat() {
            vec![sidequest_game::LoreCategory::Event, sidequest_game::LoreCategory::Character]
        } else if ctx.chase_state.is_some() {
            vec![sidequest_game::LoreCategory::Geography]
        } else {
            vec![] // default: Geography/Faction prioritized by the selector
        };
        let priority_ref: Option<&[sidequest_game::LoreCategory]> = if priority_cats.is_empty() {
            None
        } else {
            Some(&priority_cats)
        };
        let lore_budget = 500; // ~500 tokens for lore context
        let has_embeddings = ctx.lore_store.fragments_with_embeddings_count() > 0;

        // Generate query embedding for semantic search when fragments have embeddings
        let query_embedding = if has_embeddings {
            let hint = if !ctx.current_location.is_empty() {
                Some(ctx.current_location.as_str())
            } else {
                None
            };
            if let Some(hint_text) = hint {
                let config = sidequest_daemon_client::DaemonConfig::default();
                if let Ok(mut client) = sidequest_daemon_client::DaemonClient::connect(config).await {
                    let params = sidequest_daemon_client::EmbedParams { text: hint_text.to_string() };
                    match client.embed(params).await {
                        Ok(result) => Some(result.embedding),
                        Err(e) => {
                            tracing::warn!(error = %e, "lore.query_embedding_failed — falling back to category ranking");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let fallback_to_keyword = query_embedding.is_none();
        let selected = sidequest_game::select_lore_for_prompt(
            ctx.lore_store,
            lore_budget,
            priority_ref,
            query_embedding.as_deref(),
        );

        // AC-7: OTEL lore.semantic_retrieval (story 15-7)
        WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
            .field("event", "lore.semantic_retrieval")
            .field("query_hint", ctx.current_location.as_str())
            .field("fallback_to_keyword", fallback_to_keyword)
            .field("selected_count", selected.len())
            .send(ctx.state);

        // Watcher: lore retrieval breakdown (story 18-4 — Lore tab)
        let lore_summary = sidequest_game::summarize_lore_retrieval(
            ctx.lore_store,
            &selected,
            lore_budget,
            priority_ref,
        );
        WatcherEventBuilder::new("lore", WatcherEventType::LoreRetrieval)
            .field("budget", lore_summary.budget)
            .field("tokens_used", lore_summary.tokens_used)
            .field("selected_count", lore_summary.selected.len())
            .field("rejected_count", lore_summary.rejected.len())
            .field("selected", &lore_summary.selected)
            .field("rejected", &lore_summary.rejected)
            .field("total_fragments", lore_summary.total_fragments)
            .field_opt("context_hint", &lore_summary.context_hint)
            .send(ctx.state);

        if !selected.is_empty() {
            let lore_text = sidequest_game::format_lore_context(&selected);
            tracing::info!(
                fragments = selected.len(),
                tokens = selected.iter().map(|f| f.token_estimate()).sum::<usize>(),
                priority_categories = ?priority_ref,
                "rag.lore_injected_to_prompt"
            );
            state_summary.push_str("\n\n");
            state_summary.push_str(&lore_text);
        }
    }

    // Inject chase cinematography context (story 15-17)
    if let Some(ref chase_state) = ctx.chase_state {
        let chase_context = chase_state.format_context(vec![]);
        if !chase_context.is_empty() {
            state_summary.push_str("\n\n");
            state_summary.push_str(&chase_context);

            // OTEL: chase.context_injected — GM panel verification
            let beat = chase_state.current_beat(vec![]);
            let cine = sidequest_game::cinematography_for_phase(beat.phase);
            WatcherEventBuilder::new("chase", WatcherEventType::StateTransition)
                .field("event", "chase.context_injected")
                .field("phase", format!("{:?}", beat.phase))
                .field("danger_level", beat.terrain_danger)
                .field("camera", format!("{:?}", cine.camera))
                .field("sentence_range", format!("{}-{}", cine.sentence_range.0, cine.sentence_range.1))
                .send(ctx.state);

            tracing::info!(
                phase = ?beat.phase,
                danger = beat.terrain_danger,
                "chase.context_injected_to_prompt"
            );
        }
    }

    // Inject continuity corrections from the previous turn (if any)
    if !ctx.continuity_corrections.is_empty() {
        state_summary.push_str("\n\n");
        state_summary.push_str(ctx.continuity_corrections);
        tracing::info!(
            corrections_len = ctx.continuity_corrections.len(),
            "continuity.corrections_injected_to_prompt"
        );
        // Clear after injection — corrections are one-shot
        ctx.continuity_corrections.clear();
    }

    // OTEL: log prompt budget decisions
    WatcherEventBuilder::new("prompt_budget", WatcherEventType::StateTransition)
        .field("total_chars", state_summary.len())
        .field("turn_number", turn_number)
        .field("references_inventory", relevance.references_inventory)
        .field("references_ability", relevance.references_ability)
        .field("references_npc", relevance.references_npc)
        .field("references_location", relevance.references_location)
        .send(ctx.state);

    state_summary
}
