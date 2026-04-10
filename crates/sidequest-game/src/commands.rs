//! Core slash commands — /status, /inventory, /map, /save.
//!
//! Each command implements `CommandHandler` and operates as a pure function
//! of game state. No LLM calls, no async, no side effects.

use std::collections::HashMap;

use crate::slash_router::{CommandHandler, CommandResult};
use crate::state::{GameSnapshot, NpcPatch, WorldStatePatch};

/// `/status` — Shows character HP, level, class, race, location, and situation.
pub struct StatusCommand;

impl CommandHandler for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "Show your character's current condition"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        let Some(ch) = state.characters.first() else {
            return CommandResult::Error("No character found".to_string());
        };

        let mut output = format!(
            "{} — Level {} {} {}\nHP: {}/{} | AC: {}\nLocation: {} ({})",
            ch.core.name,
            ch.core.level,
            ch.race,
            ch.char_class,
            ch.core.hp,
            ch.core.max_hp,
            ch.core.ac,
            state.location,
            state.current_region,
        );

        // Bug #25: Include stat grid (Brawn, Reflexes, etc.) — previously missing
        if !ch.stats.is_empty() {
            output.push_str("\n\n");
            let mut stats: Vec<_> = ch.stats.iter().collect();
            stats.sort_by_key(|(k, _)| (*k).clone());
            for (stat, value) in &stats {
                output.push_str(&format!("  {:12} {}\n", stat, value));
            }
        }

        // Abilities
        if !ch.abilities.is_empty() {
            output.push_str("\nAbilities:\n");
            for ability in &ch.abilities {
                output.push_str(&format!(
                    "  • {} — {}\n",
                    ability.genre_description, ability.mechanical_effect
                ));
            }
        }

        if !ch.narrative_state.is_empty() {
            output.push_str(&format!("\n{}", ch.narrative_state));
        }

        CommandResult::Display(output)
    }
}

/// `/inventory` — Lists equipped items, pack contents, and gold.
pub struct InventoryCommand;

impl CommandHandler for InventoryCommand {
    fn name(&self) -> &str {
        "inventory"
    }

    fn description(&self) -> &str {
        "List your carried items"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        let Some(ch) = state.characters.first() else {
            return CommandResult::Error("No character found".to_string());
        };

        let inv = &ch.core.inventory;
        let carried: Vec<_> = inv.carried().collect();

        if carried.is_empty() && inv.gold == 0 {
            return CommandResult::Display(
                "You carry nothing of note. Your pockets are empty.".to_string(),
            );
        }

        let mut output = String::new();

        // Equipped items
        let equipped: Vec<_> = carried.iter().filter(|i| i.equipped).collect();
        output.push_str("EQUIPPED:\n");
        if equipped.is_empty() {
            output.push_str("  (nothing equipped)\n");
        } else {
            for item in &equipped {
                output.push_str(&format!("  {} — {}\n", item.name, item.description));
            }
        }

        // Pack items
        let pack: Vec<_> = carried.iter().filter(|i| !i.equipped).collect();
        output.push_str("\nPACK:\n");
        if pack.is_empty() {
            output.push_str("  (empty)\n");
        } else {
            for item in &pack {
                if item.quantity > 1 {
                    output.push_str(&format!("  {} x{}\n", item.name, item.quantity));
                } else {
                    output.push_str(&format!("  {}\n", item.name));
                }
            }
        }

        // Former possessions — quest hooks
        let lost_items: Vec<_> = inv.recoverable();
        if !lost_items.is_empty() {
            output.push_str("\nFORMER POSSESSIONS:\n");
            for item in &lost_items {
                output.push_str(&format!("  {} ({})\n", item.name, item.state));
            }
        }

        // Gold
        output.push_str(&format!("\nGold: {}", inv.gold));

        CommandResult::Display(output)
    }
}

/// `/quests` — Shows active, completed, and failed quests.
pub struct QuestsCommand;

impl CommandHandler for QuestsCommand {
    fn name(&self) -> &str {
        "quests"
    }

    fn description(&self) -> &str {
        "Show your quest log"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        if state.quest_log.is_empty() {
            return CommandResult::Display(
                "No quests recorded yet. The story is just beginning.".to_string(),
            );
        }

        let mut active = Vec::new();
        let mut completed = Vec::new();
        let mut failed = Vec::new();

        for (name, status) in &state.quest_log {
            if status.starts_with("completed") {
                completed.push((name, status));
            } else if status.starts_with("failed") {
                failed.push((name, status));
            } else {
                active.push((name, status));
            }
        }

        let mut output = String::new();

        if !active.is_empty() {
            output.push_str("ACTIVE QUESTS:\n");
            for (name, status) in &active {
                output.push_str(&format!("  {} — {}\n", name, status));
            }
        }
        if !completed.is_empty() {
            output.push_str("\nCOMPLETED:\n");
            for (name, status) in &completed {
                output.push_str(&format!("  {} — {}\n", name, status));
            }
        }
        if !failed.is_empty() {
            output.push_str("\nFAILED:\n");
            for (name, status) in &failed {
                output.push_str(&format!("  {} — {}\n", name, status));
            }
        }

        CommandResult::Display(output)
    }
}

/// `/map` — Shows discovered regions with current location marked, plus routes.
pub struct MapCommand;

impl CommandHandler for MapCommand {
    fn name(&self) -> &str {
        "map"
    }

    fn description(&self) -> &str {
        "Show discovered regions and routes"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        let mut output = String::new();

        output.push_str("REGIONS:\n");
        if state.discovered_regions.is_empty() {
            output.push_str("  No regions discovered yet.\n");
        } else {
            for region in &state.discovered_regions {
                if region == &state.current_region {
                    output.push_str(&format!("  * {} (current)\n", region));
                } else {
                    output.push_str(&format!("    {}\n", region));
                }
            }
        }

        output.push_str("\nROUTES:\n");
        if state.discovered_routes.is_empty() {
            output.push_str("  No routes discovered yet.\n");
        } else {
            for route in &state.discovered_routes {
                output.push_str(&format!("  {}\n", route));
            }
        }

        CommandResult::Display(output)
    }
}

/// `/save` — Returns a confirmation message for game state persistence.
pub struct SaveCommand;

impl CommandHandler for SaveCommand {
    fn name(&self) -> &str {
        "save"
    }

    fn description(&self) -> &str {
        "Save your game"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        let name = state
            .characters
            .first()
            .map(|ch| ch.core.name.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        CommandResult::Display(format!("Game saved for {}.", name))
    }
}

/// `/gm` — Game master commands that modify game state.
///
/// Dispatches on subcommand: set, teleport, spawn, dmg.
/// All subcommands return `StateMutation(WorldStatePatch)`.
pub struct GmCommand;

impl CommandHandler for GmCommand {
    fn name(&self) -> &str {
        "gm"
    }

    fn description(&self) -> &str {
        "Game master commands (operator only)"
    }

    fn handle(&self, _state: &GameSnapshot, args: &str) -> CommandResult {
        let (sub, sub_args) = match args.split_once(' ') {
            Some((s, a)) => (s, a.trim()),
            None => (args, ""),
        };
        match sub {
            "set" => Self::handle_set(sub_args),
            "teleport" => Self::handle_teleport(sub_args),
            "spawn" => Self::handle_spawn(sub_args),
            "dmg" => Self::handle_dmg(sub_args),
            "" => CommandResult::Error("Usage: /gm <set|teleport|spawn|dmg> [args]".to_string()),
            other => CommandResult::Error(format!("Unknown GM subcommand: {}", other)),
        }
    }
}

impl GmCommand {
    fn handle_set(args: &str) -> CommandResult {
        let (field, value) = match args.split_once(' ') {
            Some((f, v)) => (f, v),
            None if !args.is_empty() => {
                return CommandResult::Error(
                    "Usage: /gm set <field> <value>. Missing value.".to_string(),
                );
            }
            None => {
                return CommandResult::Error("Usage: /gm set <field> <value>".to_string());
            }
        };

        let mut patch = WorldStatePatch::default();
        match field {
            "location" => patch.location = Some(value.to_string()),
            "time_of_day" => patch.time_of_day = Some(value.to_string()),
            "atmosphere" => patch.atmosphere = Some(value.to_string()),
            "current_region" => patch.current_region = Some(value.to_string()),
            "active_stakes" => patch.active_stakes = Some(value.to_string()),
            other => {
                return CommandResult::Error(format!(
                    "Unknown field: '{}'. Valid fields: location, time_of_day, atmosphere, current_region, active_stakes",
                    other
                ));
            }
        }
        CommandResult::StateMutation(patch)
    }

    fn handle_teleport(args: &str) -> CommandResult {
        let (region, location) = match args.split_once(' ') {
            Some((r, l)) => (r, l),
            None if !args.is_empty() => {
                return CommandResult::Error("Usage: /gm teleport <region> <location>".to_string());
            }
            None => {
                return CommandResult::Error("Usage: /gm teleport <region> <location>".to_string());
            }
        };

        CommandResult::StateMutation(WorldStatePatch {
            location: Some(location.to_string()),
            current_region: Some(region.to_string()),
            discover_regions: Some(vec![region.to_string()]),
            ..Default::default()
        })
    }

    fn handle_spawn(args: &str) -> CommandResult {
        if args.is_empty() {
            return CommandResult::Error(
                "Usage: /gm spawn <name> [role] [personality]".to_string(),
            );
        }

        let parts: Vec<&str> = args.splitn(3, ' ').collect();
        let name = parts[0];
        let role = parts.get(1).map(|s| s.to_string());
        let personality = parts.get(2).map(|s| s.to_string());

        CommandResult::StateMutation(WorldStatePatch {
            npcs_present: Some(vec![NpcPatch {
                name: name.to_string(),
                description: None,
                personality,
                role,
                pronouns: None,
                appearance: None,
                age: None,
                build: None,
                height: None,
                distinguishing_features: None,
                location: None,
            }]),
            ..Default::default()
        })
    }

    fn handle_dmg(args: &str) -> CommandResult {
        if args.is_empty() {
            return CommandResult::Error("Usage: /gm dmg <target> <amount>".to_string());
        }

        // Find the last word as the amount, everything before is the target name
        let args_trimmed = args.trim();
        let (target, amount_str) = match args_trimmed.rsplit_once(' ') {
            Some((t, a)) => (t, a),
            None => {
                return CommandResult::Error("Usage: /gm dmg <target> <amount>".to_string());
            }
        };

        let amount: i32 = match amount_str.parse() {
            Ok(n) => n,
            Err(_) => {
                return CommandResult::Error(format!(
                    "Invalid number '{}'. Amount must be a valid integer.",
                    amount_str
                ));
            }
        };

        let mut hp_changes = HashMap::new();
        hp_changes.insert(target.to_string(), -amount);

        CommandResult::StateMutation(WorldStatePatch {
            hp_changes: Some(hp_changes),
            ..Default::default()
        })
    }
}
