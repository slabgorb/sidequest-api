//! Core slash commands — /status, /inventory, /map, /save.
//!
//! Each command implements `CommandHandler` and operates as a pure function
//! of game state. No LLM calls, no async, no side effects.

use crate::slash_router::{CommandHandler, CommandResult};
use crate::state::GameSnapshot;

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

        CommandResult::Display(format!(
            "{} — Level {} {} {}\nHP: {}/{} | AC: {}\nLocation: {} ({})\n{}",
            ch.core.name,
            ch.core.level,
            ch.race,
            ch.char_class,
            ch.core.hp,
            ch.core.max_hp,
            ch.core.ac,
            state.location,
            state.current_region,
            ch.narrative_state,
        ))
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

        if inv.items.is_empty() && inv.gold == 0 {
            return CommandResult::Display(
                "You carry nothing of note. Your pockets are as empty as the wasteland.".to_string(),
            );
        }

        let mut output = String::new();

        // Equipped items
        let equipped: Vec<_> = inv.items.iter().filter(|i| i.equipped).collect();
        output.push_str("EQUIPPED:\n");
        if equipped.is_empty() {
            output.push_str("  (nothing equipped)\n");
        } else {
            for item in &equipped {
                output.push_str(&format!("  {} — {}\n", item.name, item.description));
            }
        }

        // Pack items
        let pack: Vec<_> = inv.items.iter().filter(|i| !i.equipped).collect();
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

        // Gold
        output.push_str(&format!("\nGold: {}", inv.gold));

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
