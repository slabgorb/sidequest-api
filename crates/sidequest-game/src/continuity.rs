//! Continuity Validator — post-narration state consistency checks.
//!
//! Runs AFTER narration is received. Detects contradictions between what the
//! narrator said and what the game state actually is. Corrections are injected
//! into the NEXT turn's narrator prompt so the LLM self-corrects.

use serde::{Deserialize, Serialize};

use crate::combatant::Combatant;
use crate::state::GameSnapshot;

/// Category of state contradiction detected in narrator text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionCategory {
    /// Narrator described a location that doesn't match canonical state.
    Location,
    /// Narrator mentioned an NPC who is dead (hp == 0).
    DeadNpc,
    /// Narrator described the player using an item they don't have.
    Inventory,
    /// Narrator used time-of-day language inconsistent with current period.
    TimeOfDay,
}

/// A single contradiction between narrator text and game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// What kind of contradiction.
    pub category: ContradictionCategory,
    /// Human-readable description of the contradiction.
    pub detail: String,
    /// What the game state says the correct value is.
    pub expected: String,
}

/// Result of running all continuity checks against a narration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    /// All contradictions found.
    pub contradictions: Vec<Contradiction>,
}

impl ValidationResult {
    /// Returns true if no contradictions were found.
    pub fn is_clean(&self) -> bool {
        self.contradictions.is_empty()
    }

    /// Format contradictions as a `[STATE CORRECTIONS]` block for narrator prompt injection.
    pub fn format_corrections(&self) -> String {
        if self.contradictions.is_empty() {
            return String::new();
        }
        let mut lines = vec!["[STATE CORRECTIONS]".to_string()];
        lines.push("The following contradictions were detected in your previous narration. Correct these in your next response:".to_string());
        for c in &self.contradictions {
            lines.push(format!(
                "- {:?}: {} (expected: {})",
                c.category, c.detail, c.expected
            ));
        }
        lines.push("[/STATE CORRECTIONS]".to_string());
        lines.join("\n")
    }
}

/// Action phrases that trigger inventory validation.
const ACTION_PHRASES: &[&str] = &[
    "draw your",
    "draws their",
    "draws his",
    "draws her",
    "wield",
    "wields",
    "brandish",
    "brandishes",
];

/// Time-of-day keyword mapping: period name -> keywords that should appear.
fn time_keywords(period: &str) -> &'static [&'static str] {
    match period {
        "morning" => &["sunrise", "dawn"],
        "afternoon" => &["midday", "noon"],
        "evening" => &["dusk", "sunset"],
        "night" => &["midnight", "moonlight", "starlight"],
        _ => &[],
    }
}

/// All time-of-day keywords across all periods (for detecting mismatches).
const ALL_TIME_KEYWORDS: &[(&str, &str)] = &[
    ("sunrise", "morning"),
    ("dawn", "morning"),
    ("midday", "afternoon"),
    ("noon", "afternoon"),
    ("dusk", "evening"),
    ("sunset", "evening"),
    ("midnight", "night"),
    ("moonlight", "night"),
    ("starlight", "night"),
];

/// Validate narrator text against game state. Returns all contradictions found.
pub fn validate(narrator_text: &str, state: &GameSnapshot) -> ValidationResult {
    let text_lower = narrator_text.to_lowercase();
    let mut contradictions = Vec::new();

    check_location(&text_lower, state, &mut contradictions);
    check_dead_npcs(&text_lower, state, &mut contradictions);
    check_inventory(&text_lower, state, &mut contradictions);
    check_time_of_day(&text_lower, state, &mut contradictions);

    ValidationResult { contradictions }
}

/// Location: canonical location name must appear (case-insensitive) in narrator text.
fn check_location(text_lower: &str, state: &GameSnapshot, out: &mut Vec<Contradiction>) {
    if state.location.is_empty() {
        return;
    }
    let location_lower = state.location.to_lowercase();
    if !text_lower.contains(&location_lower) {
        out.push(Contradiction {
            category: ContradictionCategory::Location,
            detail: format!(
                "Narration does not mention the current location \"{}\"",
                state.location
            ),
            expected: state.location.clone(),
        });
    }
}

/// Dead NPC: any NPC with hp==0 mentioned by name is a contradiction.
fn check_dead_npcs(text_lower: &str, state: &GameSnapshot, out: &mut Vec<Contradiction>) {
    for npc in &state.npcs {
        if npc.hp() > 0 {
            continue;
        }
        let name_lower = npc.name().to_lowercase();
        if name_lower.is_empty() {
            continue;
        }
        if text_lower.contains(&name_lower) {
            out.push(Contradiction {
                category: ContradictionCategory::DeadNpc,
                detail: format!(
                    "Dead NPC \"{}\" (hp=0) is mentioned in the narration",
                    npc.name()
                ),
                expected: format!("{} is dead and should not appear", npc.name()),
            });
        }
    }
}

/// Inventory: if action phrases are present, at least one known item must be mentioned.
fn check_inventory(text_lower: &str, state: &GameSnapshot, out: &mut Vec<Contradiction>) {
    let has_action_phrase = ACTION_PHRASES.iter().any(|p| text_lower.contains(p));
    if !has_action_phrase {
        return;
    }

    let mut item_names: Vec<String> = Vec::new();
    for character in &state.characters {
        for item in &character.core.inventory.items {
            item_names.push(item.name.as_str().to_lowercase());
        }
    }

    if item_names.is_empty() {
        out.push(Contradiction {
            category: ContradictionCategory::Inventory,
            detail: "Narration describes wielding/drawing an item but the party has no items"
                .to_string(),
            expected: "No items in inventory".to_string(),
        });
        return;
    }

    let any_item_mentioned = item_names
        .iter()
        .any(|name| text_lower.contains(name.as_str()));
    if !any_item_mentioned {
        out.push(Contradiction {
            category: ContradictionCategory::Inventory,
            detail: "Narration describes wielding/drawing an item not found in inventory"
                .to_string(),
            expected: format!("Known items: {}", item_names.join(", ")),
        });
    }
}

/// Time-of-day: keywords in text must match current period.
fn check_time_of_day(text_lower: &str, state: &GameSnapshot, out: &mut Vec<Contradiction>) {
    if state.time_of_day.is_empty() {
        return;
    }
    let period = state.time_of_day.to_lowercase();
    let valid_keywords = time_keywords(&period);

    for &(keyword, belongs_to) in ALL_TIME_KEYWORDS {
        if text_lower.contains(keyword) && belongs_to != period {
            if !valid_keywords.contains(&keyword) {
                out.push(Contradiction {
                    category: ContradictionCategory::TimeOfDay,
                    detail: format!(
                        "Narration uses \"{}\" which implies {} but current time is {}",
                        keyword, belongs_to, state.time_of_day
                    ),
                    expected: format!("Time of day is {}", state.time_of_day),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature_core::CreatureCore;
    use crate::disposition::Disposition;
    use crate::inventory::{Inventory, Item};
    use crate::npc::Npc;
    use sidequest_protocol::NonBlankString;

    fn make_snapshot() -> GameSnapshot {
        GameSnapshot {
            location: "The Rusty Anchor".to_string(),
            time_of_day: "night".to_string(),
            ..GameSnapshot::default()
        }
    }

    fn make_npc(name: &str, hp: i32) -> Npc {
        Npc {
            core: CreatureCore {
                name: NonBlankString::new(name).unwrap(),
                description: NonBlankString::new("An NPC").unwrap(),
                personality: NonBlankString::new("Gruff").unwrap(),
                level: 1,
                hp,
                max_hp: 10,
                ac: 10,
                statuses: vec![],
                inventory: Inventory::default(),
            },
            voice_id: None,
            disposition: Disposition::new(0),
            location: None,
            pronouns: None,
            appearance: None,
            age: None,
            build: None,
            height: None,
            distinguishing_features: vec![],
            ocean: None,
        }
    }

    fn make_item(name: &str) -> Item {
        Item {
            id: NonBlankString::new(name.to_lowercase().replace(' ', "_").as_str()).unwrap(),
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test item").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value: 0,
            weight: 1.0,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.3,
            tags: vec![],
            equipped: false,
            quantity: 1,
        }
    }

    fn make_character_with_items(items: Vec<Item>) -> crate::character::Character {
        crate::character::Character {
            core: CreatureCore {
                name: NonBlankString::new("TestHero").unwrap(),
                description: NonBlankString::new("A hero").unwrap(),
                personality: NonBlankString::new("Brave").unwrap(),
                level: 1,
                hp: 10,
                max_hp: 10,
                ac: 10,
                statuses: vec![],
                inventory: Inventory { items, gold: 0 },
            },
            backstory: NonBlankString::new("Born brave").unwrap(),
            narrative_state: String::new(),
            hooks: vec![],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Human").unwrap(),
            stats: std::collections::HashMap::new(),
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
        }
    }

    // --- Location tests ---

    #[test]
    fn location_present_is_clean() {
        let state = make_snapshot();
        let result = validate("You look around the rusty anchor and see barrels.", &state);
        assert!(result.is_clean());
    }

    #[test]
    fn location_missing_is_contradiction() {
        let state = make_snapshot();
        let result = validate("You stand in a dark forest clearing.", &state);
        assert!(!result.is_clean());
        assert_eq!(
            result.contradictions[0].category,
            ContradictionCategory::Location
        );
    }

    #[test]
    fn empty_location_skips_check() {
        let mut state = make_snapshot();
        state.location = String::new();
        let result = validate("Anything goes here.", &state);
        assert!(result.is_clean());
    }

    // --- Dead NPC tests ---

    #[test]
    fn dead_npc_mentioned_is_contradiction() {
        let mut state = make_snapshot();
        state.npcs.push(make_npc("Grimjaw", 0));
        let result = validate(
            "Grimjaw greets you at the rusty anchor with a toothy grin.",
            &state,
        );
        assert!(!result.is_clean());
        assert_eq!(
            result.contradictions[0].category,
            ContradictionCategory::DeadNpc
        );
    }

    #[test]
    fn alive_npc_mentioned_is_clean() {
        let mut state = make_snapshot();
        state.npcs.push(make_npc("Grimjaw", 5));
        let result = validate(
            "Grimjaw greets you at the rusty anchor with a toothy grin.",
            &state,
        );
        assert!(result.is_clean());
    }

    #[test]
    fn dead_npc_not_mentioned_is_clean() {
        let mut state = make_snapshot();
        state.npcs.push(make_npc("Grimjaw", 0));
        let result = validate(
            "The bartender slides you a drink at the rusty anchor.",
            &state,
        );
        assert!(result.is_clean());
    }

    // --- Inventory tests ---

    #[test]
    fn action_phrase_with_known_item_is_clean() {
        let mut state = make_snapshot();
        state
            .characters
            .push(make_character_with_items(vec![make_item("Iron Sword")]));
        let result = validate("You draw your iron sword at the rusty anchor.", &state);
        assert!(result.is_clean());
    }

    #[test]
    fn action_phrase_with_unknown_item_is_contradiction() {
        let mut state = make_snapshot();
        state
            .characters
            .push(make_character_with_items(vec![make_item("Iron Sword")]));
        let result = validate("You brandish a flaming axe at the rusty anchor.", &state);
        assert!(!result.is_clean());
        assert_eq!(
            result.contradictions[0].category,
            ContradictionCategory::Inventory
        );
    }

    #[test]
    fn action_phrase_with_empty_inventory_is_contradiction() {
        let mut state = make_snapshot();
        state.characters.push(make_character_with_items(vec![]));
        let result = validate("You draw your weapon at the rusty anchor.", &state);
        assert!(!result.is_clean());
        assert_eq!(
            result.contradictions[0].category,
            ContradictionCategory::Inventory
        );
    }

    #[test]
    fn no_action_phrase_skips_inventory_check() {
        let state = make_snapshot();
        let result = validate("You look around the rusty anchor carefully.", &state);
        assert!(result.is_clean());
    }

    // --- Time-of-day tests ---

    #[test]
    fn matching_time_keyword_is_clean() {
        let state = make_snapshot(); // time_of_day = "night"
        let result = validate("The moonlight illuminates the rusty anchor.", &state);
        assert!(result.is_clean());
    }

    #[test]
    fn conflicting_time_keyword_is_contradiction() {
        let state = make_snapshot(); // time_of_day = "night"
        let result = validate(
            "The sunrise bathes the rusty anchor in golden light.",
            &state,
        );
        assert!(!result.is_clean());
        assert_eq!(
            result.contradictions[0].category,
            ContradictionCategory::TimeOfDay
        );
    }

    #[test]
    fn no_time_keywords_is_clean() {
        let state = make_snapshot();
        let result = validate("You enter the rusty anchor and order a drink.", &state);
        assert!(result.is_clean());
    }

    #[test]
    fn empty_time_of_day_skips_check() {
        let mut state = make_snapshot();
        state.time_of_day = String::new();
        let result = validate("The sunrise is beautiful at the rusty anchor.", &state);
        assert!(result.is_clean());
    }

    // --- format_corrections tests ---

    #[test]
    fn format_corrections_empty_when_clean() {
        let result = ValidationResult::default();
        assert_eq!(result.format_corrections(), "");
    }

    #[test]
    fn format_corrections_produces_block() {
        let result = ValidationResult {
            contradictions: vec![Contradiction {
                category: ContradictionCategory::Location,
                detail: "Wrong location".to_string(),
                expected: "The Tavern".to_string(),
            }],
        };
        let formatted = result.format_corrections();
        assert!(formatted.contains("[STATE CORRECTIONS]"));
        assert!(formatted.contains("[/STATE CORRECTIONS]"));
        assert!(formatted.contains("Wrong location"));
        assert!(formatted.contains("The Tavern"));
    }

    // --- Multiple contradictions ---

    #[test]
    fn multiple_contradictions_collected() {
        let mut state = make_snapshot();
        state.npcs.push(make_npc("Grimjaw", 0));
        let result = validate("Grimjaw waves as you arrive at the sunrise market.", &state);
        assert!(result.contradictions.len() >= 2);
        let categories: Vec<_> = result.contradictions.iter().map(|c| &c.category).collect();
        assert!(categories.contains(&&ContradictionCategory::Location));
        assert!(categories.contains(&&ContradictionCategory::DeadNpc));
    }
}
