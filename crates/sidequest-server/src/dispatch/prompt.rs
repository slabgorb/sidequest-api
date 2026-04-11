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
                    .send();
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

    // Death directive — the narrator MUST describe the character's death
    if ctx.snapshot.player_dead || *ctx.hp <= 0 {
        state_summary.push_str(
            "\n\n⚠️ CHARACTER IS DEAD (HP 0). The character has fallen in combat. \
             Narrate the death scene — describe how they fell, what killed them, \
             and the finality of it. Do NOT continue the adventure. Do NOT let \
             the character act, move, or speak. The session is over. End with \
             a brief epitaph or closing line.",
        );
    }

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

            // Always inject agency constraint — even single-player.
            // Bug #1/#3: narrator puppeted PCs when constraint was multiplayer-only.
            if !other_pcs.is_empty() {
                state_summary.push_str(&format!("\n\nParty: {}.", other_pcs.join(", ")));
                if !co_located_names.is_empty() {
                    state_summary
                        .push_str(&format!(" Co-located: {}.", co_located_names.join(", ")));
                    // Enrich with mechanical context for co-located PCs so the narrator
                    // can write mechanically-aware party interactions.
                    let co_located_pids = ss.co_located_players(ctx.player_id);
                    for pid in &co_located_pids {
                        if let Some(ps) = ss.players.get(pid.as_str()) {
                            if let Some(ref name) = ps.character_name {
                                state_summary.push_str(&format!(
                                    "\n  {} — {} Lv{}, HP {}/{}",
                                    name,
                                    if ps.character_class.is_empty() {
                                        "Unknown"
                                    } else {
                                        &ps.character_class
                                    },
                                    ps.character_level,
                                    ps.character_hp,
                                    ps.character_max_hp,
                                ));
                            }
                        }
                    }

                    // OTEL: party context injection
                    crate::WatcherEventBuilder::new(
                        "party_context",
                        crate::WatcherEventType::StateTransition,
                    )
                    .field("event", "party_context_injected")
                    .field("co_located_count", co_located_pids.len())
                    .field("co_located_names", co_located_names.join(", ").as_str())
                    .send();
                }
            }
            // PC roster for the agency constraint — always includes the active player.
            let mut all_pc_names: Vec<String> = vec![ctx.char_name.to_string()];
            all_pc_names.extend(other_pcs.iter().cloned());
            if turn_number <= 3 {
                // Full rules for early turns
                state_summary.push_str(&format!(
                    "\n\nPLAYER AGENCY — ABSOLUTE RULE:\n\
                     Player characters: {}\n\
                     Do NOT write dialogue, actions, thoughts, or internal state for ANY player character.\n\
                     Players control their OWN characters. You control the WORLD, NPCs, and narration only.\n\
                     Do NOT script physical interactions between player characters (nudging, grabbing, etc.).\n\
                     PERSPECTIVE: Third-person omniscient. All characters named explicitly. Never use 'you'.",
                    all_pc_names.join(", ")
                ));
            } else {
                // Compressed reminder after turn 3
                state_summary.push_str(&format!(
                    " PLAYER AGENCY: Player characters: {}. Do not write dialogue/actions/thoughts for them. No PC-to-PC physical scripting. Third-person only.",
                    all_pc_names.join(", ")
                ));
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
    tracing::info!(
        carried_count = ctx.inventory.item_count(),
        ledger_size = ctx.inventory.ledger_size(),
        gold = ctx.inventory.gold,
        "prompt.inventory_check — building inventory section"
    );
    if ctx.inventory.item_count() > 0 {
        if relevance.references_inventory {
            // Full inventory with descriptions and rules
            state_summary
                .push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
            state_summary.push_str("\nThe player currently possesses EXACTLY these items:");
            for item in ctx.inventory.carried() {
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
            let equipped: Vec<String> = ctx
                .inventory
                .carried()
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
                ctx.inventory.item_count(),
                equipped_str,
                ctx.inventory.gold
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
        state_summary.push_str("Reference active quests when narratively relevant. Quest state changes are handled via the quest_update tool.\n");
    }

    // Inject character's discovered knowledge so narrator can reference it.
    // Limits to most recent 20 facts to stay within token budget.
    if let Some(ref cj) = ctx.character_json {
        if let Some(facts) = cj.get("known_facts").and_then(|v| v.as_array()) {
            let relevant: Vec<_> = facts.iter().rev().take(20).collect();
            if !relevant.is_empty() {
                state_summary
                    .push_str("\n\n[CHARACTER KNOWLEDGE — facts this character has learned]\n");
                for fact in &relevant {
                    if let Some(content) = fact.get("content").and_then(|c| c.as_str()) {
                        let cat = fact
                            .get("category")
                            .and_then(|c| c.as_str())
                            .unwrap_or("unknown");
                        state_summary.push_str(&format!("- [{}] {}\n", cat, content));
                    }
                }
                tracing::info!(
                    facts_injected = relevant.len(),
                    total_facts = facts.len(),
                    "rag.known_facts_injected"
                );
                WatcherEventBuilder::new("rag", WatcherEventType::SubsystemExerciseSummary)
                    .field("event", "rag.known_facts_injected")
                    .field("injected", relevant.len())
                    .field("total", facts.len())
                    .send();
            }
        }
    }

    // Resource state injection — reads directly from ResourcePool (phase 2
    // of resource consolidation). Each pool carries its own label/bounds/
    // decay from init_resource_pools(), so no need to cross-reference a
    // separate declarations vec.
    if !ctx.snapshot.resources.is_empty() {
        state_summary.push_str("\n\nGENRE RESOURCES — Current State:\n");
        // Sort by name for stable prompt output — HashMap iteration order
        // is non-deterministic and the narrator's cache key depends on
        // exact prompt bytes.
        let mut pools: Vec<&sidequest_game::ResourcePool> =
            ctx.snapshot.resources.values().collect();
        pools.sort_by(|a, b| a.name.cmp(&b.name));
        for pool in pools {
            let vol_label = if pool.voluntary {
                "voluntary"
            } else {
                "involuntary"
            };
            // Fall back to name if label is empty (migrated old save with
            // minimal pool entry that hasn't been upsert-populated yet).
            let display_label = if pool.label.is_empty() {
                &pool.name
            } else {
                &pool.label
            };
            let mut line = format!(
                "{}: {}/{} ({})",
                display_label, pool.current, pool.max, vol_label
            );
            if pool.decay_per_turn.abs() > f64::EPSILON {
                line.push_str(&format!(", decay {}/turn", pool.decay_per_turn.abs()));
            }
            state_summary.push_str(&format!("- {}\n", line));
        }
        state_summary.push_str("When narrative events affect these resources, include resource_deltas in your JSON block.\n");
    }

    // Available confrontation types — always injected so the narrator knows what
    // encounter types it can trigger via the confrontation field in game_patch.
    if !ctx.confrontation_defs.is_empty() {
        state_summary.push_str("\n\n=== AVAILABLE CONFRONTATIONS ===\n");
        state_summary.push_str(
            "When a structured encounter begins (combat, card game, chase, standoff, etc.), ",
        );
        state_summary
            .push_str("emit a confrontation field in your game_patch with one of these types:\n");
        for def in ctx.confrontation_defs.iter() {
            state_summary.push_str(&format!(
                "- type: \"{}\" — {} (category: {})\n",
                def.confrontation_type, def.label, def.category
            ));
        }
        state_summary.push_str("=== END AVAILABLE CONFRONTATIONS ===\n");
    }

    // Structured encounter context via format_encounter_context() (story 28-4)
    if let Some(ref enc) = ctx.snapshot.encounter {
        if let Some(def) =
            crate::find_confrontation_def(&ctx.confrontation_defs, &enc.encounter_type)
        {
            WatcherEventBuilder::new("encounter", WatcherEventType::AgentSpanOpen)
                .field("action", "context_injected")
                .field("encounter_type", &enc.encounter_type)
                .field(
                    "phase",
                    enc.structured_phase
                        .map(|p| format!("{:?}", p))
                        .unwrap_or_else(|| "unphased".to_string()),
                )
                .field("beat_count", def.beats.len())
                .field(
                    "metric",
                    format!("{}: {}", enc.metric.name, enc.metric.current),
                )
                .send();
            state_summary.push_str("\n\n");
            state_summary.push_str(&enc.format_encounter_context(def));
        } else {
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("action", "confrontation_def_missing")
                .field("encounter_type", &enc.encounter_type)
                .field("available_defs", ctx.confrontation_defs.len())
                .send();
        }
    }

    // Inject available confrontation types so the narrator knows what encounters
    // it can initiate via the "confrontation" game_patch field.
    // Only inject when no encounter is active (active encounter context is above).
    if ctx.snapshot.encounter.is_none() && !ctx.confrontation_defs.is_empty() {
        state_summary.push_str("\n\nAVAILABLE ENCOUNTER TYPES:\n");
        state_summary.push_str("When combat, a chase, or another confrontation begins, include \"confrontation\": \"<type>\" in your game_patch. Valid types for this genre:\n");
        for def in ctx.confrontation_defs.iter() {
            state_summary.push_str(&format!(
                "- \"{}\" ({}, {})\n",
                def.confrontation_type, def.label, def.category
            ));
        }
        state_summary.push_str("Only emit confrontation on the turn the encounter STARTS.\n");

        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("action", "available_types_injected")
            .field("count", ctx.confrontation_defs.len())
            .field(
                "types",
                ctx.confrontation_defs
                    .iter()
                    .map(|d| d.confrontation_type.clone())
                    .collect::<Vec<_>>(),
            )
            .send();
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
        // Filter out lore anchor placeholders ("faction: auto-filled from genre pack")
        if let Some(hooks) = cj.get("hooks").and_then(|h| h.as_array()) {
            let hook_strs: Vec<&str> = hooks
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.contains("auto-filled"))
                .collect();
            if !hook_strs.is_empty() {
                if relevance.references_ability {
                    state_summary.push_str("\n\nABILITY CONSTRAINTS — THIS IS A HARD RULE:\n");
                    state_summary.push_str("The PLAYER CHARACTER can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt. Do NOT apply these abilities to NPCs.\n");
                    state_summary.push_str("Allowed abilities:\n");
                    for h in &hook_strs {
                        state_summary.push_str(&format!("- {}\n", h));
                    }
                } else {
                    state_summary.push_str(&format!("\nAbilities: {}.", hook_strs.join(", ")));
                }
                // Bug #12: Always inject proactive mutation narration — not just when
                // the player references an ability. The narrator should weave mutations
                // into the scene whenever the context creates a natural opportunity,
                // even (especially) when the player doesn't explicitly invoke them.
                state_summary.push_str(
                    " PROACTIVE ABILITY NARRATION: When the scene naturally creates an opportunity for the character's abilities/mutations to manifest, weave subtle sensory hints into the narration (e.g., psychic whispers, glowing skin, heightened senses). Do NOT wait for the player to invoke them."
                );
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
            let hook: String = ctx
                .world_context
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
    // Only current location + discovered regions — the narrator doesn't need
    // the full world atlas to narrate the current scene.
    if !ctx.discovered_regions.is_empty() {
        state_summary.push_str("\n\nKNOWN LOCATIONS IN THIS WORLD:\n");
        state_summary.push_str("Use ONLY these location names when referring to places the party has visited or heard about. Do NOT invent new settlement names.\n");
        for region in ctx.discovered_regions.iter() {
            state_summary.push_str(&format!("- {}\n", region));
        }
    }
    // Inject unvisited cartography locations, filtered by adjacency when a world
    // graph is available. Without a graph, cap at 5 to avoid dumping the full atlas.
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if !ss.region_names.is_empty() {
                // Collect undiscovered region IDs
                let undiscovered: Vec<&str> = ss
                    .region_names
                    .iter()
                    .filter(|(region_id, _)| {
                        !ctx.discovered_regions
                            .iter()
                            .any(|r| r.to_lowercase() == *region_id)
                    })
                    .map(|(region_id, _)| region_id.as_str())
                    .collect();

                if !undiscovered.is_empty() {
                    // Filter by adjacency if world graph is available
                    let filtered: Vec<&str> = if let Some(ref wg) = ctx.world_graph {
                        let neighbors: Vec<&str> = wg.neighbors(ctx.current_location).collect();
                        undiscovered
                            .into_iter()
                            .filter(|r| {
                                neighbors.iter().any(|n| {
                                    n.to_lowercase() == r.to_lowercase()
                                        || n.to_lowercase().contains(&r.to_lowercase())
                                        || r.to_lowercase().contains(&n.to_lowercase())
                                })
                            })
                            .collect()
                    } else {
                        // No graph — cap at 5 nearest (by order in the list)
                        undiscovered.into_iter().take(5).collect()
                    };

                    if !filtered.is_empty() {
                        if ctx.discovered_regions.is_empty() {
                            state_summary.push_str("\n\nNEARBY LOCATIONS (from cartography):\n");
                            state_summary.push_str(
                                "Use these canonical location names. Do NOT invent new ones.\n",
                            );
                        } else {
                            state_summary.push_str("Nearby locations (not yet visited):\n");
                        }
                        for region_id in &filtered {
                            state_summary.push_str(&format!("- {}\n", region_id));
                        }
                    }
                }
            }
        }
    }

    // Room graph navigation — inject current room + available exits
    if !ctx.rooms.is_empty() {
        if let Some(current_room) = ctx
            .rooms
            .iter()
            .find(|r| r.id == *ctx.current_location || r.name == *ctx.current_location)
        {
            state_summary.push_str("\n\nROOM NAVIGATION (room-graph mode):\n");
            state_summary.push_str(&format!(
                "Current room: {} — {}\n",
                current_room.name,
                current_room.description.as_deref().unwrap_or("")
            ));
            if !current_room.exits.is_empty() {
                state_summary.push_str("Exits:\n");
                for exit in &current_room.exits {
                    state_summary.push_str(&format!(
                        "- {} → {}\n",
                        exit.display_name(),
                        exit.target()
                    ));
                }
            }
            state_summary.push_str("When the player moves through an exit, update the location header to the target room name.\n");
            state_summary.push_str("IMPORTANT: When the player enters a new room, always end your narration by describing the visible exits and 2-3 obvious actions or points of interest. Players navigate by exits — without them, every turn becomes 'where can I go?'\n");

            WatcherEventBuilder::new("navigation", WatcherEventType::StateTransition)
                .field("mode", "room_graph")
                .field("current_room", &current_room.id)
                .field("exit_count", current_room.exits.len())
                .send();
        }
    }

    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Inject tone context from narrative axes (story F2/F10)
    // Tone directives are static — only inject on first 3 turns (establishing session).
    // After that they're in conversation history via the persistent narrator session.
    if turn_number <= 3 {
        if let Some(ref ac) = ctx.axes_config {
            let tone_text = sidequest_game::format_tone_context(ac, ctx.axis_values);
            if !tone_text.is_empty() {
                state_summary.push_str(&tone_text);
            }
        }
    }

    // Resolve and inject character abilities from affinity tiers (story 15-15)
    if let Some(ch) = ctx.snapshot.characters.first() {
        let genre_affinities = &ctx.genre_affinities;
        let all_abilities = sidequest_game::resolve_abilities(&ch.affinities, &|name, tier| {
            genre_affinities
                .iter()
                .find(|a| a.name == name)
                .and_then(|a| a.unlocks.as_ref())
                .and_then(|u| match tier {
                    0 => u.tier_0.as_ref(),
                    1 => u.tier_1.as_ref(),
                    2 => u.tier_2.as_ref(),
                    3 => u.tier_3.as_ref(),
                    _ => None,
                })
                .map(|t| t.abilities.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default()
        });
        if !all_abilities.is_empty() {
            let abilities_text = sidequest_game::format_abilities_context(&all_abilities);
            state_summary.push_str(&abilities_text);

            let tiers_active = ch.affinities.iter().filter(|a| a.tier > 0).count();
            WatcherEventBuilder::new("abilities", WatcherEventType::StateTransition)
                .field("event", "abilities.resolved")
                .field("count", all_abilities.len())
                .field("tiers_active", tiers_active)
                .field("ability_names", all_abilities.join(", "))
                .send();
            tracing::info!(
                count = all_abilities.len(),
                tiers_active = tiers_active,
                "abilities.resolved"
            );
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
    let npc_context = build_npc_registry_context_budgeted(
        ctx.npc_registry,
        turn_number,
        relevance.references_npc,
    );
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Story 7-9: Scenario context injection — tension, clues, NPC suspicions.
    // Uses format_narrator_context() from ScenarioState to give the narrator
    // awareness of the active scenario's state.
    if let Some(ref scenario_state) = ctx.snapshot.scenario_state {
        if !scenario_state.is_resolved() {
            let scenario_context = scenario_state.format_narrator_context(&ctx.snapshot.npcs);
            if !scenario_context.is_empty() {
                state_summary.push_str("\n\n[SCENARIO STATE]\n");
                state_summary.push_str(&scenario_context);
                state_summary.push('\n');

                WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                    .field("event", "scenario.context_injected")
                    .field("tension", format!("{:.2}", scenario_state.tension()))
                    .field("clues_discovered", scenario_state.discovered_clues().len())
                    .field("resolved", scenario_state.is_resolved())
                    .send();
            }
        }
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        // Prioritize lore categories based on current game state
        let priority_cats: Vec<sidequest_game::LoreCategory> = if ctx.in_combat() {
            vec![
                sidequest_game::LoreCategory::Event,
                sidequest_game::LoreCategory::Character,
            ]
        } else if ctx.in_chase() {
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
                               // Phase 1: short lock to read the embedded-fragment count. Must not
                               // hold the lock across the query-embedding daemon call below —
                               // that would re-introduce the dispatch blocking we just fixed.
        let has_embeddings = {
            let store = ctx.lore_store.lock().await;
            store.fragments_with_embeddings_count() > 0
        };

        // Phase 2: generate query embedding for semantic search when
        // fragments have embeddings. Runs WITHOUT holding the lore store
        // lock so concurrent embed worker writes aren't blocked by the
        // prompt pipeline.
        let query_embedding = if has_embeddings {
            let hint = if !ctx.current_location.is_empty() {
                Some(ctx.current_location.as_str())
            } else {
                None
            };
            if let Some(hint_text) = hint {
                let config = sidequest_daemon_client::DaemonConfig::default();
                if let Ok(mut client) = sidequest_daemon_client::DaemonClient::connect(config).await
                {
                    let params = sidequest_daemon_client::EmbedParams {
                        text: hint_text.to_string(),
                    };
                    match client.embed(params).await {
                        Ok(result) => Some(result.embedding),
                        Err(e) => {
                            // Surface the failure to the GM panel — without
                            // this watcher event, a wedged daemon would
                            // silently downgrade every prompt to keyword
                            // ranking and the operator would have no
                            // signal that semantic retrieval was disabled.
                            tracing::warn!(error = %e, "lore.query_embedding_failed — falling back to category ranking");
                            WatcherEventBuilder::new("lore", WatcherEventType::ValidationWarning)
                                .field("event", "lore.query_embedding_failed")
                                .field("error_kind", "embed_failed")
                                .field("error", e.to_string().as_str())
                                .field("query_hint", hint_text)
                                .send();
                            None
                        }
                    }
                } else {
                    WatcherEventBuilder::new("lore", WatcherEventType::ValidationWarning)
                        .field("event", "lore.query_embedding_failed")
                        .field("error_kind", "daemon_unreachable")
                        .field("query_hint", hint_text)
                        .send();
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let fallback_to_keyword = query_embedding.is_none();
        // Phase 3: re-acquire the lock for the duration of `selected`'s
        // lifetime — `select_lore_for_prompt` returns `Vec<&LoreFragment>`
        // that borrows from the store, so the guard must stay alive until
        // we're done reading those refs.
        {
            let store = ctx.lore_store.lock().await;
            let selected = sidequest_game::select_lore_for_prompt(
                &store,
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
                .send();

            // Watcher: lore retrieval breakdown (story 18-4 — Lore tab)
            let lore_summary = sidequest_game::summarize_lore_retrieval(
                &store,
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
                .send();

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
    }

    // Inject conlang vocabulary — learned language knowledge (story 15-19)
    {
        // Acquire the lore store lock for the scope that holds the
        // borrowed `lang_fragments` refs.
        let store = ctx.lore_store.lock().await;
        let lang_fragments = sidequest_game::query_all_language_knowledge(&store, ctx.player_id);
        if !lang_fragments.is_empty() {
            let conlang_text =
                sidequest_game::format_language_knowledge_for_prompt(&lang_fragments);
            if !conlang_text.is_empty() {
                state_summary.push_str(&conlang_text);

                // Collect unique language IDs for OTEL
                let language_ids: Vec<&str> = lang_fragments
                    .iter()
                    .filter_map(|f| f.metadata().get("language_id").map(|s| s.as_str()))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                WatcherEventBuilder::new("conlang", WatcherEventType::StateTransition)
                    .field("event", "conlang_knowledge_injected")
                    .field("vocab_count", lang_fragments.len())
                    .field("language_count", language_ids.len())
                    .field("languages", language_ids.join(", "))
                    .send();

                tracing::info!(
                    vocab_count = lang_fragments.len(),
                    language_count = language_ids.len(),
                    "conlang.knowledge_injected_to_prompt"
                );
            }
        }
    }

    // Story 15-19: Inject genre pack name banks for narrator reference.
    // Provides pre-generated conlang names the narrator can use for consistency.
    for bank in &ctx.name_banks {
        let bank_text = sidequest_game::format_name_bank_for_prompt(bank, 20);
        if !bank_text.is_empty() {
            state_summary.push_str("\n\n");
            state_summary.push_str(&bank_text);
        }
    }

    // Chase cinematography context injection removed in story 28-9.
    // Encounter context is now injected via format_encounter_context() (story 28-4).

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
        .send();

    state_summary
}
