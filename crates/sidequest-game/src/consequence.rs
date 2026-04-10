//! Wish Consequence Engine — detects power-grab actions and assigns ironic
//! consequence categories for the narrator to flavor.
//!
//! Feature F9: The "Genie Wish" system. When a player attempts an overpowered
//! action (kill all, teleport, summon weapon, etc.), the engine doesn't refuse —
//! it grants the wish with an ironic twist. The category rotates mechanically;
//! the narrator fills in the creative consequence description.

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

/// Consequence assignment engine for power-grab actions.
///
/// Detection is handled upstream by the LLM preprocessor (`is_power_grab` field).
/// This engine assigns rotating consequence categories and builds prompt context.
pub struct WishConsequenceEngine {
    rotation_counter: usize,
}

impl WishConsequenceEngine {
    /// Create a new engine with rotation counter starting at 0.
    pub fn new() -> Self {
        Self {
            rotation_counter: 0,
        }
    }

    /// Create an engine seeded with a rotation counter (for persistence across turns).
    pub fn with_counter(rotation_counter: usize) -> Self {
        Self { rotation_counter }
    }

    /// Assign a consequence to a power-grab action.
    ///
    /// `is_power_grab` comes from the LLM preprocessor's classification.
    /// Returns `Some(GenieWish)` if power grab, `None` otherwise.
    pub fn evaluate(
        &mut self,
        player_name: &str,
        action_text: &str,
        is_power_grab: bool,
    ) -> Option<GenieWish> {
        if !is_power_grab {
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
            Some(ConsequenceCategory::Backfire) => {
                "BACKFIRE (the wish works but goes wrong in an unexpected way)"
            }
            Some(ConsequenceCategory::Attention) => {
                "ATTENTION (the wish draws unwanted attention — enemies, cosmic entities, rivals)"
            }
            Some(ConsequenceCategory::Cost) => {
                "COST (the wish exacts a steep price — health, items, relationships)"
            }
            Some(ConsequenceCategory::Curse) => {
                "CURSE (the wish triggers a lasting curse or side effect)"
            }
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
    fn power_grab_true_returns_wish() {
        let mut engine = WishConsequenceEngine::new();
        let result = engine.evaluate("Thorin", "wish for unlimited gold", true);
        assert!(result.is_some());
        let wish = result.unwrap();
        assert_eq!(wish.wisher_name, "Thorin");
        assert_eq!(wish.status, WishStatus::Pending);
        assert!(wish.consequence_category.is_some());
    }

    #[test]
    fn power_grab_false_returns_none() {
        let mut engine = WishConsequenceEngine::new();
        assert!(engine.evaluate("Normal", "open the door", false).is_none());
    }

    #[test]
    fn with_counter_seeds_rotation() {
        let mut engine = WishConsequenceEngine::with_counter(2);
        let w = engine.evaluate("A", "power grab", true).unwrap();
        assert_eq!(w.consequence_category, Some(ConsequenceCategory::Cost));
    }

    #[test]
    fn rotation_cycles_through_categories() {
        let mut engine = WishConsequenceEngine::new();

        let w1 = engine.evaluate("A", "grab 1", true).unwrap();
        assert_eq!(w1.consequence_category, Some(ConsequenceCategory::Backfire));

        let w2 = engine.evaluate("B", "grab 2", true).unwrap();
        assert_eq!(
            w2.consequence_category,
            Some(ConsequenceCategory::Attention)
        );

        let w3 = engine.evaluate("C", "grab 3", true).unwrap();
        assert_eq!(w3.consequence_category, Some(ConsequenceCategory::Cost));

        let w4 = engine.evaluate("D", "grab 4", true).unwrap();
        assert_eq!(w4.consequence_category, Some(ConsequenceCategory::Curse));

        let w5 = engine.evaluate("E", "grab 5", true).unwrap();
        assert_eq!(w5.consequence_category, Some(ConsequenceCategory::Backfire));
    }

    #[test]
    fn rotation_counter_only_increments_on_power_grab() {
        let mut engine = WishConsequenceEngine::new();
        assert_eq!(engine.rotation_counter(), 0);

        engine.evaluate("A", "normal action", false);
        assert_eq!(engine.rotation_counter(), 0);

        engine.evaluate("A", "power grab", true);
        assert_eq!(engine.rotation_counter(), 1);

        engine.evaluate("A", "another normal", false);
        assert_eq!(engine.rotation_counter(), 1);
    }

    #[test]
    fn wish_has_unique_ids() {
        let mut engine = WishConsequenceEngine::new();
        let w1 = engine.evaluate("A", "grab 1", true).unwrap();
        let w2 = engine.evaluate("A", "grab 2", true).unwrap();
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
        assert_eq!(
            roundtripped.consequence_category,
            Some(ConsequenceCategory::Curse)
        );
    }
}
