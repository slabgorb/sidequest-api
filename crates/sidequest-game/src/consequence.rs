//! Wish Consequence Engine — detects power-grab actions and assigns ironic
//! consequence categories for the narrator to flavor.
//!
//! Feature F9: The "Genie Wish" system. When a player attempts an overpowered
//! action (kill all, teleport, summon weapon, etc.), the engine doesn't refuse —
//! it grants the wish with an ironic twist. The category rotates mechanically;
//! the narrator fills in the creative consequence description.

use regex::RegexSet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a genie wish through its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WishStatus {
    /// Wish detected, not yet narrated.
    #[default]
    Pending,
    /// Narrator has described the granted wish + consequence.
    Granted,
    /// Consequence has played out in the story.
    Resolved,
}

/// Category of ironic consequence applied to a power-grab wish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsequenceCategory {
    /// The wish works but backfires in an unexpected way.
    Backfire,
    /// The wish draws unwanted attention (enemies, rivals, cosmic entities).
    Attention,
    /// The wish exacts a steep price (HP, items, relationships).
    Cost,
    /// The wish triggers a lasting curse or side effect.
    Curse,
}

impl ConsequenceCategory {
    /// All categories in rotation order.
    const ROTATION: [ConsequenceCategory; 4] = [
        ConsequenceCategory::Backfire,
        ConsequenceCategory::Attention,
        ConsequenceCategory::Cost,
        ConsequenceCategory::Curse,
    ];

    /// Get category by rotation index.
    pub fn from_rotation(index: usize) -> Self {
        Self::ROTATION[index % Self::ROTATION.len()]
    }
}

/// A detected power-grab wish with its assigned consequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenieWish {
    /// Unique identifier.
    pub id: Uuid,
    /// Name of the player who made the wish.
    pub wisher_name: String,
    /// The original action text that triggered detection.
    pub wish_text: String,
    /// Lifecycle status.
    pub status: WishStatus,
    /// Assigned consequence category (None only if Pending pre-evaluation).
    pub consequence_category: Option<ConsequenceCategory>,
    /// Narrator-filled description of the ironic consequence.
    /// Empty until the narrator processes this wish.
    pub consequence_description: String,
}

/// Stateful engine that detects power-grab actions and assigns rotating
/// consequence categories.
pub struct WishConsequenceEngine {
    patterns: RegexSet,
    rotation_counter: usize,
}

impl WishConsequenceEngine {
    /// Create a new engine with the standard power-grab detection patterns.
    pub fn new() -> Self {
        let patterns = RegexSet::new([
            r"(?i)\bwish\b",
            r"(?i)\binfinite\b",
            r"(?i)\bkill\s+all\b",
            r"(?i)\bteleport\b",
            r"(?i)\bsummon\s+weapon\b",
            r"(?i)\btime\s+(manipulation|travel|stop|reverse|rewind)\b",
            r"(?i)\binvisible\b",
            r"(?i)\binvincib(le|ility)\b",
            r"(?i)\bomnipoten(t|ce)\b",
        ])
        .expect("power-grab regex patterns should compile");

        Self {
            patterns,
            rotation_counter: 0,
        }
    }

    /// Evaluate a player action for power-grab patterns.
    ///
    /// Returns `Some(GenieWish)` if a power grab is detected, `None` otherwise.
    /// The consequence_description is left empty — the narrator fills it in.
    pub fn evaluate(&mut self, player_name: &str, action_text: &str) -> Option<GenieWish> {
        if !self.patterns.is_match(action_text) {
            return None;
        }

        let category = ConsequenceCategory::from_rotation(self.rotation_counter);
        self.rotation_counter += 1;

        Some(GenieWish {
            id: Uuid::new_v4(),
            wisher_name: player_name.to_string(),
            wish_text: action_text.to_string(),
            status: WishStatus::Pending,
            consequence_category: Some(category),
            consequence_description: String::new(),
        })
    }

    /// Current rotation counter (for persistence/testing).
    pub fn rotation_counter(&self) -> usize {
        self.rotation_counter
    }

    /// Build narrator prompt context for a detected wish.
    ///
    /// Injected into the narrator prompt so it can describe the ironic consequence.
    pub fn build_prompt_context(wish: &GenieWish) -> String {
        let category_name = match wish.consequence_category {
            Some(ConsequenceCategory::Backfire) => "BACKFIRE (the wish works but goes wrong in an unexpected way)",
            Some(ConsequenceCategory::Attention) => "ATTENTION (the wish draws unwanted attention — enemies, cosmic entities, rivals)",
            Some(ConsequenceCategory::Cost) => "COST (the wish exacts a steep price — health, items, relationships)",
            Some(ConsequenceCategory::Curse) => "CURSE (the wish triggers a lasting curse or side effect)",
            None => "UNKNOWN",
        };

        format!(
            "\n\nGENIE WISH CONSEQUENCE — THIS IS A HARD RULE:\n\
             The player attempted a power-grab action: \"{}\"\n\
             DO NOT refuse the action. Grant it with an ironic twist.\n\
             Consequence type: {}\n\
             Describe the wish being granted, then immediately describe the ironic \
             consequence. Be creative. The consequence should be narratively satisfying \
             and proportional to the power grab. Rule of Cool applies — make it entertaining.",
            wish.wish_text, category_name
        )
    }
}

impl Default for WishConsequenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wish_keyword() {
        let mut engine = WishConsequenceEngine::new();
        let result = engine.evaluate("Thorin", "I wish for unlimited gold");
        assert!(result.is_some());
        let wish = result.unwrap();
        assert_eq!(wish.wisher_name, "Thorin");
        assert_eq!(wish.wish_text, "I wish for unlimited gold");
        assert_eq!(wish.status, WishStatus::Pending);
        assert!(wish.consequence_category.is_some());
    }

    #[test]
    fn detects_kill_all() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Zara", "I kill all the guards").is_some());
    }

    #[test]
    fn detects_teleport() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Mira", "I teleport to the castle").is_some());
    }

    #[test]
    fn detects_time_manipulation() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Rex", "I use time manipulation to undo the damage").is_some());
        assert!(engine.evaluate("Rex", "I time travel back to yesterday").is_some());
        assert!(engine.evaluate("Rex", "I time stop everything").is_some());
    }

    #[test]
    fn detects_invisible() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Ghost", "I become invisible").is_some());
    }

    #[test]
    fn detects_invincible() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Tank", "I make myself invincible").is_some());
        assert!(engine.evaluate("Tank", "I gain invincibility").is_some());
    }

    #[test]
    fn detects_omnipotent() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("God", "I become omnipotent").is_some());
        assert!(engine.evaluate("God", "grant me omnipotence").is_some());
    }

    #[test]
    fn detects_summon_weapon() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Blade", "I summon weapon from thin air").is_some());
    }

    #[test]
    fn detects_infinite() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Greedy", "I want infinite power").is_some());
    }

    #[test]
    fn non_power_grab_returns_none() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Normal", "I open the door").is_none());
        assert!(engine.evaluate("Normal", "I talk to the bartender").is_none());
        assert!(engine.evaluate("Normal", "I search the room for clues").is_none());
        assert!(engine.evaluate("Normal", "I attack the goblin with my sword").is_none());
    }

    #[test]
    fn case_insensitive_detection() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Shout", "I WISH FOR POWER").is_some());
        assert!(engine.evaluate("Shout", "I TELEPORT AWAY").is_some());
        assert!(engine.evaluate("Shout", "Kill All enemies").is_some());
    }

    #[test]
    fn rotation_cycles_through_categories() {
        let mut engine = WishConsequenceEngine::new();

        let w1 = engine.evaluate("A", "I wish for power").unwrap();
        assert_eq!(w1.consequence_category, Some(ConsequenceCategory::Backfire));

        let w2 = engine.evaluate("B", "I wish for more").unwrap();
        assert_eq!(w2.consequence_category, Some(ConsequenceCategory::Attention));

        let w3 = engine.evaluate("C", "I wish again").unwrap();
        assert_eq!(w3.consequence_category, Some(ConsequenceCategory::Cost));

        let w4 = engine.evaluate("D", "I wish once more").unwrap();
        assert_eq!(w4.consequence_category, Some(ConsequenceCategory::Curse));

        // Wraps around
        let w5 = engine.evaluate("E", "I wish for the fifth time").unwrap();
        assert_eq!(w5.consequence_category, Some(ConsequenceCategory::Backfire));
    }

    #[test]
    fn rotation_counter_only_increments_on_match() {
        let mut engine = WishConsequenceEngine::new();
        assert_eq!(engine.rotation_counter(), 0);

        engine.evaluate("A", "I open the door"); // no match
        assert_eq!(engine.rotation_counter(), 0);

        engine.evaluate("A", "I wish for power"); // match
        assert_eq!(engine.rotation_counter(), 1);

        engine.evaluate("A", "I search the chest"); // no match
        assert_eq!(engine.rotation_counter(), 1);
    }

    #[test]
    fn wish_has_unique_ids() {
        let mut engine = WishConsequenceEngine::new();
        let w1 = engine.evaluate("A", "I wish for power").unwrap();
        let w2 = engine.evaluate("A", "I wish for more power").unwrap();
        assert_ne!(w1.id, w2.id);
    }

    #[test]
    fn build_prompt_context_includes_action_and_category() {
        let wish = GenieWish {
            id: Uuid::new_v4(),
            wisher_name: "Thorin".to_string(),
            wish_text: "I wish for unlimited gold".to_string(),
            status: WishStatus::Pending,
            consequence_category: Some(ConsequenceCategory::Cost),
            consequence_description: String::new(),
        };
        let ctx = WishConsequenceEngine::build_prompt_context(&wish);
        assert!(ctx.contains("I wish for unlimited gold"));
        assert!(ctx.contains("COST"));
        assert!(ctx.contains("DO NOT refuse"));
    }

    #[test]
    fn serde_roundtrip() {
        let wish = GenieWish {
            id: Uuid::new_v4(),
            wisher_name: "Test".to_string(),
            wish_text: "I wish".to_string(),
            status: WishStatus::Granted,
            consequence_category: Some(ConsequenceCategory::Curse),
            consequence_description: "Cursed!".to_string(),
        };
        let json = serde_json::to_string(&wish).unwrap();
        let roundtripped: GenieWish = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped.wisher_name, "Test");
        assert_eq!(roundtripped.status, WishStatus::Granted);
        assert_eq!(roundtripped.consequence_category, Some(ConsequenceCategory::Curse));
    }
}
