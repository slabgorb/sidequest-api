//! Guest NPC players — human-controlled NPC characters with limited agency.
//!
//! Story 8-7: Players can join a session as an existing NPC with restricted
//! action set. The narrator treats guest-controlled NPCs as semi-autonomous.
//!
//! **Stub module** — types compile but methods are unimplemented (RED phase).

use std::collections::HashSet;

/// Role of a player in a multiplayer session.
///
/// Full players have unrestricted agency; guest NPC players are limited
/// to a configurable set of action categories.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PlayerRole {
    /// Full player — unrestricted agency.
    Full,
    /// Guest controlling an NPC — restricted to allowed actions.
    GuestNpc {
        /// Name of the NPC being controlled.
        npc_name: String,
        /// Which action categories this guest is allowed to perform.
        allowed_actions: HashSet<ActionCategory>,
    },
}

impl PlayerRole {
    /// Check if this role permits a given action category.
    pub fn can_perform(&self, _action: &ActionCategory) -> bool {
        todo!("8-7: implement action permission check")
    }

    /// Default set of allowed actions for guest NPC players.
    ///
    /// Returns {Dialogue, Movement, Examine} per story context AC-3.
    pub fn default_guest_actions() -> HashSet<ActionCategory> {
        todo!("8-7: implement default guest actions")
    }

    /// Whether this role is a guest NPC (not a full player).
    pub fn is_guest(&self) -> bool {
        todo!("8-7: implement is_guest check")
    }
}

/// Categories of player actions, used for guest NPC restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActionCategory {
    /// Speaking, questioning, persuading.
    Dialogue,
    /// Walking, running, climbing, moving between areas.
    Movement,
    /// Looking at, inspecting, searching.
    Examine,
    /// Fighting, attacking, defending.
    Combat,
    /// Managing items, trading, equipping.
    Inventory,
}

/// Error type for guest NPC action validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ActionError {
    /// The action category is restricted for this guest NPC.
    #[error("restricted action: {category:?} is not allowed for this guest NPC")]
    RestrictedAction {
        /// Which category was attempted.
        category: ActionCategory,
    },
    /// The player is not in the session.
    #[error("player not in session")]
    NotInSession,
}

/// Narrator prompt annotation for a guest-controlled NPC.
///
/// Generates the `[GUEST NPC: ...]` block that tells the narrator to treat
/// this character as semi-autonomous (controlled by a guest player).
#[derive(Debug)]
pub struct NarratorTag {
    npc_name: String,
}

impl NarratorTag {
    /// Create a narrator tag for a guest-controlled NPC.
    pub fn for_guest(npc_name: &str) -> Self {
        Self {
            npc_name: npc_name.to_string(),
        }
    }

    /// Render the tag as a prompt string for the narrator.
    pub fn to_prompt_string(&self) -> String {
        todo!("8-7: implement narrator prompt tag")
    }
}

/// Context for a guest NPC player within a session.
///
/// Wraps the player ID, NPC name, and role. Provides validation and
/// perception mode queries.
pub struct GuestNpcContext {
    player_id: String,
    npc_name: String,
    role: PlayerRole,
}

impl GuestNpcContext {
    /// Create a new guest NPC context.
    pub fn new(
        player_id: String,
        npc_name: String,
        allowed_actions: HashSet<ActionCategory>,
    ) -> Self {
        Self {
            player_id,
            npc_name: npc_name.clone(),
            role: PlayerRole::GuestNpc {
                npc_name,
                allowed_actions,
            },
        }
    }

    /// The player ID.
    pub fn player_id(&self) -> &str {
        &self.player_id
    }

    /// The NPC name this guest is controlling.
    pub fn npc_name(&self) -> &str {
        &self.npc_name
    }

    /// The player role.
    pub fn role(&self) -> &PlayerRole {
        &self.role
    }

    /// Validate that an action is permitted for this guest.
    pub fn validate_action(&self, _action: &ActionCategory) -> Result<(), ActionError> {
        todo!("8-7: implement action validation")
    }

    /// Generate the narrator tag for this guest NPC.
    pub fn narrator_tag(&self) -> NarratorTag {
        NarratorTag::for_guest(&self.npc_name)
    }

    /// Whether this guest NPC uses inverted perception mode.
    ///
    /// Guest NPCs see from the NPC's perspective (motives, secrets),
    /// while protagonist players see only visible NPC behavior.
    pub fn perception_mode_inverted(&self) -> bool {
        todo!("8-7: implement inverted perception check")
    }

    /// Description of the guest NPC's perception mode for prompt composition.
    pub fn perception_description(&self) -> String {
        todo!("8-7: implement perception description")
    }

    /// List available NPCs that a guest could control.
    pub fn available_npcs(npc_names: &[String]) -> Vec<String> {
        todo!("8-7: implement available NPC listing")
    }

    /// Merge guest input with AI disposition using weighted average.
    ///
    /// `guest_weight` is clamped to [0.0, 1.0]. The AI weight is `1.0 - guest_weight`.
    /// Returns the merged disposition value as an integer.
    pub fn merge_disposition(guest_input: i32, ai_disposition: i32, guest_weight: f64) -> i32 {
        todo!("8-7: implement disposition merging")
    }
}
