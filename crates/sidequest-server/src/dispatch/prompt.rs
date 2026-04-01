//! Narrator prompt context builder — assembles state summary for the LLM.

use std::collections::HashMap;

use crate::npc_context::build_npc_registry_context;
use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Build the full state_summary string for the narrator prompt.
/// Includes trope seeding, party roster, location constraints, inventory, quests,
/// chase state, abilities, world context, regions, tone, history, NPCs, lore, and
/// continuity corrections.
#[tracing::instrument(name = "turn.build_prompt_context", skip_all)]
pub(crate) async fn build_prompt_context(ctx: &mut DispatchContext<'_>) -> String {
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
                state_summary.push_str("\n\nPLAYER-CONTROLLED CHARACTERS IN THE PARTY:\n");
                state_summary
                    .push_str("The following characters are controlled by OTHER human players:\n");
                for name in &other_pcs {
                    state_summary.push_str(&format!("- {}\n", name));
                }
                if !co_located_names.is_empty() {
                    state_summary.push_str(&format!(
                        "\nCO-LOCATION — HARD RULE: The following party members are RIGHT HERE with the acting player: {}. \
                         They are physically present at the SAME location. The narrator MUST acknowledge their presence \
                         in the scene. Do NOT narrate them as being elsewhere or arriving from somewhere else. \
                         They are already here.\n",
                        co_located_names.join(", ")
                    ));
                }
                state_summary.push_str(concat!(
                    "\n\nPLAYER AGENCY — ABSOLUTE RULE (violations break the game):\n",
                    "You MUST NOT write dialogue, actions, thoughts, feelings, gestures, or internal ",
                    "state for ANY player character — including the acting player beyond their stated action.\n",
                    "FORBIDDEN examples:\n",
                    "- \"Laverne holds up their power glove. 'I've got the strong hand covered.'\" (writing dialogue FOR a player)\n",
                    "- \"Shirley nudges Laverne with an elbow\" (scripting PC-to-PC physical interaction)\n",
                    "- \"Kael's heart races as he...\" (writing internal state for a player)\n",
                    "ALLOWED examples:\n",
                    "- \"Laverne is nearby, power glove faintly humming.\" (describing presence without action)\n",
                    "- \"The other party members are within earshot.\" (acknowledging presence)\n",
                    "Players control their OWN characters. You control the WORLD, NPCs, and narration only.",
                ));
                state_summary.push_str(
                    "\n\nPERSPECTIVE MODE: Third-person omniscient. \
                     You are narrating for multiple players simultaneously. \
                     Do NOT use 'you' for any character — including the acting player. \
                     All characters are named explicitly in third-person. \
                     Correct: 'Mira surveys the gantry. Kael moves to cover.' \
                     Wrong: 'You survey the gantry.'",
                );
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

    // Inventory constraint — the narrator must respect the character sheet
    let equipped_count = ctx.inventory.items.iter().filter(|i| i.equipped).count();
    tracing::debug!(
        items = ctx.inventory.items.len(),
        equipped = equipped_count,
        gold = ctx.inventory.gold,
        "narrator_prompt.inventory_constraint — injecting character sheet"
    );
    state_summary.push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
    if !ctx.inventory.items.is_empty() {
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
        state_summary.push_str("\nThe player has NO items. If the player claims to use any item, the narrator MUST reject it — they have nothing in their possession yet.");
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

    // Include character abilities and mutations so the narrator knows what
    // the character can and cannot do (prevents hallucinated abilities).
    if let Some(ref cj) = ctx.character_json {
        // Extract hooks (narrative abilities, mutations, etc.)
        if let Some(hooks) = cj.get("hooks").and_then(|h| h.as_array()) {
            let hook_strs: Vec<&str> = hooks.iter().filter_map(|v| v.as_str()).collect();
            if !hook_strs.is_empty() {
                state_summary.push_str("\n\nABILITY CONSTRAINTS — THIS IS A HARD RULE:\n");
                state_summary.push_str("The character can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt. Do NOT grant the character abilities they do not have.\n");
                state_summary.push_str("Allowed abilities:\n");
                for h in &hook_strs {
                    state_summary.push_str(&format!("- {}\n", h));
                }
                state_summary.push_str("If the player attempts to use an ability NOT listed above, describe the attempt failing or reframe it as a non-supernatural action.\n");
                state_summary.push_str("PROACTIVE MUTATION NARRATION: When the scene naturally creates an opportunity for the character's abilities/mutations to be relevant (sensory input, danger, social situations), weave them into the narration subtly. A psychic character might catch stray thoughts; a bioluminescent character's skin might flicker in darkness. Don't force it every turn, but don't ignore mutations either — they define who the character IS.\n");
            }
        }
        // Extract backstory
        if let Some(backstory) = cj.get("backstory").and_then(|b| b.as_str()) {
            state_summary.push_str(&format!("\nBackstory: {}", backstory));
        }
        // Extract class and race for narrator awareness
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
                tracing::debug!(pronouns = %pronouns, "narrator_prompt.pronouns — injected into state_summary");
            }
        }
    }

    if !ctx.world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(ctx.world_context);
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

    // Bug 17: Include recent narration history so the narrator maintains continuity
    if !ctx.narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT CONVERSATION HISTORY (multiple players, most recent last):\nEntries are tagged with [CharacterName]. Only narrate for the ACTING player — do not continue another player's scene:\n");
        // Include at most the last 10 turns to stay within context limits
        let start = ctx.narration_history.len().saturating_sub(10);
        for entry in &ctx.narration_history[start..] {
            state_summary.push_str(entry);
            state_summary.push('\n');
        }
    }

    // Inject NPC registry so the narrator maintains identity consistency
    let npc_context = build_npc_registry_context(ctx.npc_registry);
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        let context_hint = if !ctx.current_location.is_empty() {
            Some(ctx.current_location.as_str())
        } else {
            None
        };
        let lore_budget = 500; // ~500 tokens for lore context
        let selected =
            sidequest_game::select_lore_for_prompt(ctx.lore_store, lore_budget, context_hint);
        if !selected.is_empty() {
            let lore_text = sidequest_game::format_lore_context(&selected);
            tracing::info!(
                fragments = selected.len(),
                tokens = selected.iter().map(|f| f.token_estimate()).sum::<usize>(),
                hint = ?context_hint,
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

    state_summary
}
