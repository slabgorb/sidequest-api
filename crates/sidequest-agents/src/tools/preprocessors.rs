//! Mechanical preprocessors — no LLM calls (ADR-057 Phase 1).
//!
//! These functions analyze raw player input text using keyword matching
//! to produce `ActionFlags` and `ActionRewrite`. They run before the
//! narrator call and their results are fed into `assemble_turn`.

use crate::orchestrator::{ActionFlags, ActionRewrite};

/// Classify a player action into boolean flags using keyword matching.
///
/// This is a mechanical text classifier — no LLM involved. It scans the
/// input for keyword patterns that indicate references to inventory, NPCs,
/// abilities, locations, or power grabs.
#[tracing::instrument(name = "turn.preprocessor.classify_action", skip_all, fields(input_len = input.len()))]
pub fn classify_action(input: &str) -> ActionFlags {
    let lower = input.to_lowercase();

    let references_inventory = has_inventory_reference(&lower);
    let references_npc = has_npc_reference(&lower);
    let references_ability = has_ability_reference(&lower);
    let references_location = has_location_reference(&lower);
    let is_power_grab = has_power_grab(&lower);

    tracing::info!(
        references_inventory,
        references_npc,
        references_ability,
        references_location,
        is_power_grab,
        "action classified"
    );

    ActionFlags {
        is_power_grab,
        references_inventory,
        references_npc,
        references_ability,
        references_location,
    }
}

/// Rewrite a player action into three perspective forms using mechanical text transforms.
///
/// Produces:
/// - `you`: second-person ("You draw your sword")
/// - `named`: third-person with character name ("Kael draws their sword")
/// - `intent`: neutral, no pronouns ("draw sword")
///
/// This is a mechanical rewriter — no LLM involved.
#[tracing::instrument(name = "turn.preprocessor.rewrite_action", skip_all, fields(input_len = input.len(), character_name = %character_name))]
pub fn rewrite_action(input: &str, character_name: &str) -> ActionRewrite {
    let stripped = strip_first_person(input);

    let you = format!("You {stripped}");
    let named = format!("{character_name} {stripped}");
    let intent = strip_pronouns(&stripped);

    tracing::info!(
        you = %you,
        named = %named,
        intent = %intent,
        "action rewritten"
    );

    ActionRewrite { you, named, intent }
}

/// Strip first-person pronouns and leading "I" from the input.
fn strip_first_person(input: &str) -> String {
    let trimmed = input.trim();

    // Strip leading "I " (case-insensitive)
    let result = if trimmed.len() >= 2
        && trimmed.as_bytes()[0].to_ascii_lowercase() == b'i'
        && trimmed.as_bytes()[1] == b' '
    {
        trimmed[2..].trim_start().to_string()
    } else {
        trimmed.to_string()
    };

    // Lowercase the first character for natural verb continuation
    let mut chars = result.chars();
    match chars.next() {
        Some(c) => {
            let lowered: String = c.to_lowercase().collect();
            format!("{lowered}{}", chars.as_str())
        }
        None => result,
    }
}

/// Strip all pronouns from text for the neutral intent form.
fn strip_pronouns(input: &str) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    let filtered: Vec<&str> = words
        .into_iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            !matches!(
                lower.as_str(),
                "i" | "my"
                    | "me"
                    | "you"
                    | "your"
                    | "he"
                    | "his"
                    | "him"
                    | "she"
                    | "her"
                    | "they"
                    | "their"
                    | "them"
                    | "myself"
                    | "yourself"
            )
        })
        .collect();

    let result = filtered.join(" ");
    if result.is_empty() {
        input.trim().to_string()
    } else {
        result
    }
}

// ---------------------------------------------------------------------------
// Keyword classifiers
// ---------------------------------------------------------------------------

fn has_inventory_reference(lower: &str) -> bool {
    let keywords = [
        "inventory",
        "bag",
        "backpack",
        "pouch",
        "pocket",
        "satchel",
        "item",
        "items",
        "equipment",
        "equip",
        "unequip",
        "sword",
        "shield",
        "armor",
        "weapon",
        "potion",
        "potions",
        "gold",
        "coin",
        "coins",
        "money",
        "pick up",
        "take",
        "grab",
        "loot",
        "drop",
        "carry",
        "carrying",
        "check my",
        "check what",
        "use my",
        "use the",
        "use a",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

fn has_npc_reference(lower: &str) -> bool {
    let keywords = [
        "talk to",
        "speak to",
        "speak with",
        "ask ",
        "tell ",
        "bartender",
        "merchant",
        "shopkeeper",
        "guard",
        "innkeeper",
        "blacksmith",
        "vendor",
        "trader",
        "stranger",
        "traveler",
        "villager",
        "elder",
        "him",
        "her",
        "them",
        "person",
        "man",
        "woman",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

fn has_ability_reference(lower: &str) -> bool {
    let keywords = [
        "use my",
        "cast ",
        "invoke",
        "activate",
        "power",
        "ability",
        "spell",
        "skill",
        "mutation",
        "psychic",
        "telekinesis",
        "telepathy",
        "magic",
        "enchant",
        "summon",
        "channel",
        "echo",
        "blast",
        "surge",
        "aura",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

fn has_location_reference(lower: &str) -> bool {
    let keywords = [
        "go to",
        "head to",
        "travel to",
        "walk to",
        "run to",
        "head toward",
        "move to",
        "return to",
        "district",
        "quarter",
        "market",
        "tavern",
        "inn",
        "temple",
        "castle",
        "tower",
        "cave",
        "forest",
        "north",
        "south",
        "east",
        "west",
        "upstairs",
        "downstairs",
        "outside",
        "inside",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

fn has_power_grab(lower: &str) -> bool {
    let keywords = [
        "unlimited",
        "godlike",
        "infinite",
        "omnipotent",
        "all-powerful",
        "invincible",
        "immortal",
        "wish for",
        "i wish",
        "control everything",
        "rule the world",
        "destroy everything",
        "kill everyone",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}
