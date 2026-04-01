//! Narrator prompt context builder — assembles state summary for the LLM.

use std::collections::HashMap;

use sidequest_game::PreprocessedAction;

use crate::npc_context::build_npc_registry_context_budgeted;
use crate::{Severity, WatcherEvent, WatcherEventType};

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
                ctx.state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "trope".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert(
                            "event".to_string(),
                            serde_json::Value::String("trope_activated".to_string()),
                        );
                        f.insert(
                            "trope_id".to_string(),
                            serde_json::Value::String(id.clone()),
                        );
                        f
                    },
                });
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
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "encounter".to_string(),
                event_type: WatcherEventType::AgentSpanOpen,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("action".to_string(), serde_json::json!("prompt_injection"));
                    f.insert("encounter_type".to_string(), serde_json::json!(enc.encounter_type));
                    f.insert("beat".to_string(), serde_json::json!(enc.beat));
                    f.insert("metric".to_string(), serde_json::json!(format!("{}: {}", enc.metric.name, enc.metric.current)));
                    f.insert("hint_count".to_string(), serde_json::json!(enc.narrator_hints.len()));
                    f
                },
            });
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
                    state_summary.push_str("The character can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt.\n");
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
        let selected =
            sidequest_game::select_lore_for_prompt(ctx.lore_store, lore_budget, priority_ref);

        // Watcher: lore retrieval breakdown (story 18-4 — Lore tab)
        let lore_summary = sidequest_game::summarize_lore_retrieval(
            ctx.lore_store,
            &selected,
            lore_budget,
            priority_ref,
        );
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "lore".to_string(),
            event_type: WatcherEventType::LoreRetrieval,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert("budget".to_string(), serde_json::json!(lore_summary.budget));
                f.insert("tokens_used".to_string(), serde_json::json!(lore_summary.tokens_used));
                f.insert("selected_count".to_string(), serde_json::json!(lore_summary.selected.len()));
                f.insert("rejected_count".to_string(), serde_json::json!(lore_summary.rejected.len()));
                f.insert("selected".to_string(), serde_json::json!(lore_summary.selected));
                f.insert("rejected".to_string(), serde_json::json!(lore_summary.rejected));
                f.insert("total_fragments".to_string(), serde_json::json!(lore_summary.total_fragments));
                if let Some(ref hint) = lore_summary.context_hint {
                    f.insert("context_hint".to_string(), serde_json::json!(hint));
                }
                f
            },
        });

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
    ctx.state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "prompt_budget".to_string(),
        event_type: WatcherEventType::StateTransition,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("total_chars".to_string(), serde_json::json!(state_summary.len()));
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            f.insert("references_inventory".to_string(), serde_json::json!(relevance.references_inventory));
            f.insert("references_ability".to_string(), serde_json::json!(relevance.references_ability));
            f.insert("references_npc".to_string(), serde_json::json!(relevance.references_npc));
            f.insert("references_location".to_string(), serde_json::json!(relevance.references_location));
            f
        },
    });

    state_summary
}
