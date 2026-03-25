//! Format helpers for agent context sections.
//!
//! Ported from Python's `format_helpers.py`. These functions produce
//! human-readable text blocks for injection into agent prompts.

/// Format a character summary block.
pub fn character_block(name: &str, hp: i32, max_hp: i32, level: i32) -> String {
    format!("**{name}** (Level {level}) — HP: {hp}/{max_hp}")
}

/// Format a location summary block.
pub fn location_block(region: &str, area: &str) -> String {
    format!("**Location:** {area}, {region}")
}

/// Format an NPC summary block.
pub fn npc_block(name: &str, attitude: &str) -> String {
    format!("**{name}** [{attitude}]")
}

/// Format an inventory summary.
pub fn inventory_summary(items: &[String]) -> String {
    if items.is_empty() {
        "Inventory: No items".to_string()
    } else {
        let list = items.join(", ");
        format!("Inventory: {list}")
    }
}
