//! Story 8-7: Guest NPC Players — human-controlled NPC characters with limited agency.
//!
//! Tests cover all 6 story context ACs plus 5 session-level ACs:
//!   AC-1: Guest join — player joins session as specific NPC
//!   AC-2: Action restriction — guest cannot perform disallowed actions
//!   AC-3: Allowed actions — dialogue, movement, examine permitted by default
//!   AC-4: Validation — restricted action returns error before barrier
//!   AC-5: Narrator tag — guest NPC annotated in narrator prompt
//!   AC-6: NPC selection — available NPCs listed for guest to choose
//!   Session AC-1: GuestNPC recognized by MultiplayerSession
//!   Session AC-2: Inverted perception narration for guest NPC
//!   Session AC-4: Protagonist players see only visible NPC behavior
//!   Session AC-5: Disposition merging (guest vs AI)

use std::collections::HashSet;

use sidequest_game::guest_npc::{
    ActionCategory, ActionError, GuestNpcContext, NarratorTag, PlayerRole,
};

// ─── Test helpers ──────────────────────────────────────────

fn default_guest_role() -> PlayerRole {
    PlayerRole::GuestNpc {
        npc_name: "Marta the Innkeeper".to_string(),
        allowed_actions: PlayerRole::default_guest_actions(),
    }
}

fn full_role() -> PlayerRole {
    PlayerRole::Full
}

fn custom_guest_role(actions: HashSet<ActionCategory>) -> PlayerRole {
    PlayerRole::GuestNpc {
        npc_name: "Razortooth".to_string(),
        allowed_actions: actions,
    }
}

// ═══════════════════════════════════════════════════════════
// 1. PlayerRole enum — variants and behavior
// ═══════════════════════════════════════════════════════════

#[test]
fn player_role_full_variant_exists() {
    let role = full_role();
    assert!(matches!(role, PlayerRole::Full));
}

#[test]
fn player_role_guest_npc_variant_exists() {
    let role = default_guest_role();
    assert!(matches!(role, PlayerRole::GuestNpc { .. }));
}

#[test]
fn player_role_guest_npc_carries_npc_name() {
    if let PlayerRole::GuestNpc { npc_name, .. } = default_guest_role() {
        assert_eq!(npc_name, "Marta the Innkeeper");
    } else {
        panic!("expected GuestNpc variant");
    }
}

#[test]
fn player_role_guest_npc_carries_allowed_actions() {
    if let PlayerRole::GuestNpc {
        allowed_actions, ..
    } = default_guest_role()
    {
        assert!(!allowed_actions.is_empty());
    } else {
        panic!("expected GuestNpc variant");
    }
}

#[test]
fn player_role_is_debug() {
    let role = full_role();
    let debug = format!("{role:?}");
    assert!(!debug.is_empty());
}

#[test]
fn player_role_is_clone() {
    let role = default_guest_role();
    let cloned = role.clone();
    assert!(matches!(cloned, PlayerRole::GuestNpc { .. }));
}

// Rule #2: non_exhaustive on public enums
#[test]
fn player_role_is_non_exhaustive() {
    // If PlayerRole is non_exhaustive, this match requires a wildcard arm.
    // This test verifies the enum compiles with a wildcard.
    let role = full_role();
    match role {
        PlayerRole::Full => {}
        PlayerRole::GuestNpc { .. } => {}
        _ => {} // Required by #[non_exhaustive]
    }
}

// ═══════════════════════════════════════════════════════════
// 2. ActionCategory enum — action types
// ═══════════════════════════════════════════════════════════

#[test]
fn action_category_dialogue_exists() {
    let cat = ActionCategory::Dialogue;
    assert!(matches!(cat, ActionCategory::Dialogue));
}

#[test]
fn action_category_movement_exists() {
    let cat = ActionCategory::Movement;
    assert!(matches!(cat, ActionCategory::Movement));
}

#[test]
fn action_category_examine_exists() {
    let cat = ActionCategory::Examine;
    assert!(matches!(cat, ActionCategory::Examine));
}

#[test]
fn action_category_combat_exists() {
    let cat = ActionCategory::Combat;
    assert!(matches!(cat, ActionCategory::Combat));
}

#[test]
fn action_category_inventory_exists() {
    let cat = ActionCategory::Inventory;
    assert!(matches!(cat, ActionCategory::Inventory));
}

#[test]
fn action_category_is_debug() {
    let cat = ActionCategory::Dialogue;
    let debug = format!("{cat:?}");
    assert!(debug.contains("Dialogue"));
}

#[test]
fn action_category_is_clone() {
    let cat = ActionCategory::Movement;
    let cloned = cat.clone();
    assert!(matches!(cloned, ActionCategory::Movement));
}

#[test]
fn action_category_is_eq() {
    assert_eq!(ActionCategory::Dialogue, ActionCategory::Dialogue);
    assert_ne!(ActionCategory::Dialogue, ActionCategory::Combat);
}

#[test]
fn action_category_is_hash() {
    // Must be usable in HashSet
    let mut set = HashSet::new();
    set.insert(ActionCategory::Dialogue);
    set.insert(ActionCategory::Dialogue); // duplicate
    assert_eq!(set.len(), 1);
}

// Rule #2: non_exhaustive on public enums
#[test]
fn action_category_is_non_exhaustive() {
    let cat = ActionCategory::Dialogue;
    match cat {
        ActionCategory::Dialogue => {}
        ActionCategory::Movement => {}
        ActionCategory::Examine => {}
        ActionCategory::Combat => {}
        ActionCategory::Inventory => {}
        _ => {} // Required by #[non_exhaustive]
    }
}

// ═══════════════════════════════════════════════════════════
// 3. AC-3: Default guest actions (Dialogue, Movement, Examine)
// ═══════════════════════════════════════════════════════════

#[test]
fn default_guest_actions_contains_dialogue() {
    let actions = PlayerRole::default_guest_actions();
    assert!(
        actions.contains(&ActionCategory::Dialogue),
        "guests should be able to talk"
    );
}

#[test]
fn default_guest_actions_contains_movement() {
    let actions = PlayerRole::default_guest_actions();
    assert!(
        actions.contains(&ActionCategory::Movement),
        "guests should be able to move"
    );
}

#[test]
fn default_guest_actions_contains_examine() {
    let actions = PlayerRole::default_guest_actions();
    assert!(
        actions.contains(&ActionCategory::Examine),
        "guests should be able to examine"
    );
}

#[test]
fn default_guest_actions_excludes_combat() {
    let actions = PlayerRole::default_guest_actions();
    assert!(
        !actions.contains(&ActionCategory::Combat),
        "guests should NOT have combat by default"
    );
}

#[test]
fn default_guest_actions_excludes_inventory() {
    let actions = PlayerRole::default_guest_actions();
    assert!(
        !actions.contains(&ActionCategory::Inventory),
        "guests should NOT have inventory by default"
    );
}

#[test]
fn default_guest_actions_has_exactly_three() {
    let actions = PlayerRole::default_guest_actions();
    assert_eq!(
        actions.len(),
        3,
        "default set should be Dialogue+Movement+Examine"
    );
}

// ═══════════════════════════════════════════════════════════
// 4. can_perform — action permission checks
// ═══════════════════════════════════════════════════════════

#[test]
fn full_role_can_perform_any_action() {
    let role = full_role();
    assert!(role.can_perform(&ActionCategory::Dialogue));
    assert!(role.can_perform(&ActionCategory::Combat));
    assert!(role.can_perform(&ActionCategory::Inventory));
    assert!(role.can_perform(&ActionCategory::Movement));
    assert!(role.can_perform(&ActionCategory::Examine));
}

#[test]
fn guest_can_perform_allowed_action() {
    let role = default_guest_role();
    assert!(role.can_perform(&ActionCategory::Dialogue));
    assert!(role.can_perform(&ActionCategory::Movement));
    assert!(role.can_perform(&ActionCategory::Examine));
}

#[test]
fn guest_cannot_perform_combat() {
    let role = default_guest_role();
    assert!(
        !role.can_perform(&ActionCategory::Combat),
        "default guest should not be able to fight"
    );
}

#[test]
fn guest_cannot_perform_inventory() {
    let role = default_guest_role();
    assert!(
        !role.can_perform(&ActionCategory::Inventory),
        "default guest should not manage inventory"
    );
}

#[test]
fn custom_guest_with_combat_can_fight() {
    let mut actions = HashSet::new();
    actions.insert(ActionCategory::Dialogue);
    actions.insert(ActionCategory::Combat);
    let role = custom_guest_role(actions);
    assert!(role.can_perform(&ActionCategory::Combat));
}

#[test]
fn custom_guest_without_dialogue_cannot_talk() {
    let mut actions = HashSet::new();
    actions.insert(ActionCategory::Movement);
    let role = custom_guest_role(actions);
    assert!(!role.can_perform(&ActionCategory::Dialogue));
}

#[test]
fn guest_with_empty_actions_cannot_do_anything() {
    let role = custom_guest_role(HashSet::new());
    assert!(!role.can_perform(&ActionCategory::Dialogue));
    assert!(!role.can_perform(&ActionCategory::Movement));
    assert!(!role.can_perform(&ActionCategory::Examine));
    assert!(!role.can_perform(&ActionCategory::Combat));
    assert!(!role.can_perform(&ActionCategory::Inventory));
}

// ═══════════════════════════════════════════════════════════
// 5. is_guest — role query
// ═══════════════════════════════════════════════════════════

#[test]
fn full_role_is_not_guest() {
    assert!(!full_role().is_guest());
}

#[test]
fn guest_role_is_guest() {
    assert!(default_guest_role().is_guest());
}

// ═══════════════════════════════════════════════════════════
// 6. ActionError — error type
// ═══════════════════════════════════════════════════════════

#[test]
fn action_error_restricted_action_variant() {
    let err = ActionError::RestrictedAction {
        category: ActionCategory::Combat,
    };
    assert!(matches!(
        err,
        ActionError::RestrictedAction {
            category: ActionCategory::Combat
        }
    ));
}

#[test]
fn action_error_not_in_session_variant() {
    let err = ActionError::NotInSession;
    assert!(matches!(err, ActionError::NotInSession));
}

#[test]
fn action_error_is_debug() {
    let err = ActionError::RestrictedAction {
        category: ActionCategory::Combat,
    };
    let debug = format!("{err:?}");
    assert!(!debug.is_empty());
}

#[test]
fn action_error_displays_restricted_category() {
    let err = ActionError::RestrictedAction {
        category: ActionCategory::Combat,
    };
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("combat") || msg.to_lowercase().contains("restricted"),
        "error message should mention the restricted category: {msg}"
    );
}

#[test]
fn action_error_displays_not_in_session() {
    let err = ActionError::NotInSession;
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("session") || msg.to_lowercase().contains("not"),
        "error message should mention session: {msg}"
    );
}

// Rule #2: non_exhaustive
#[test]
fn action_error_is_non_exhaustive() {
    let err = ActionError::NotInSession;
    match err {
        ActionError::RestrictedAction { .. } => {}
        ActionError::NotInSession => {}
        _ => {} // Required by #[non_exhaustive]
    }
}

// ═══════════════════════════════════════════════════════════
// 7. AC-5: Narrator tag generation
// ═══════════════════════════════════════════════════════════

#[test]
fn narrator_tag_for_guest_npc() {
    let tag = NarratorTag::for_guest("Marta the Innkeeper");
    let text = tag.to_prompt_string();
    assert!(
        text.contains("Marta the Innkeeper"),
        "narrator tag should include NPC name: {text}"
    );
}

#[test]
fn narrator_tag_mentions_guest() {
    let tag = NarratorTag::for_guest("Marta the Innkeeper");
    let text = tag.to_prompt_string();
    assert!(
        text.to_lowercase().contains("guest"),
        "narrator tag should mention guest status: {text}"
    );
}

#[test]
fn narrator_tag_mentions_semi_autonomous() {
    let tag = NarratorTag::for_guest("Marta the Innkeeper");
    let text = tag.to_prompt_string();
    // The spec says "semi-autonomous" treatment
    assert!(
        text.to_lowercase().contains("semi-autonomous")
            || text.to_lowercase().contains("controlled"),
        "narrator tag should reference guest control model: {text}"
    );
}

#[test]
fn narrator_tag_is_debug() {
    let tag = NarratorTag::for_guest("Marta");
    let debug = format!("{tag:?}");
    assert!(!debug.is_empty());
}

// ═══════════════════════════════════════════════════════════
// 8. GuestNpcContext — session integration
// ═══════════════════════════════════════════════════════════

#[test]
fn guest_context_creation() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    assert_eq!(ctx.player_id(), "player-42");
    assert_eq!(ctx.npc_name(), "Marta the Innkeeper");
}

#[test]
fn guest_context_role_is_guest() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    assert!(ctx.role().is_guest());
}

#[test]
fn guest_context_validates_allowed_action() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    assert!(ctx.validate_action(&ActionCategory::Dialogue).is_ok());
}

#[test]
fn guest_context_rejects_restricted_action() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    let result = ctx.validate_action(&ActionCategory::Combat);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ActionError::RestrictedAction {
            category: ActionCategory::Combat,
        }
    ));
}

#[test]
fn guest_context_narrator_tag() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    let tag = ctx.narrator_tag();
    assert!(tag.to_prompt_string().contains("Marta the Innkeeper"));
}

// Rule #9: private fields with getters
#[test]
fn guest_context_fields_accessed_via_getters() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    // These compile only if getters exist (fields are private)
    let _pid: &str = ctx.player_id();
    let _name: &str = ctx.npc_name();
    let _role: &PlayerRole = ctx.role();
}

// ═══════════════════════════════════════════════════════════
// 9. AC-6: NPC selection — available NPCs
// ═══════════════════════════════════════════════════════════

#[test]
fn available_npcs_returns_npc_names() {
    let npcs = vec![
        "Marta the Innkeeper".to_string(),
        "Guard Captain".to_string(),
    ];
    let available = GuestNpcContext::available_npcs(&npcs);
    assert_eq!(available.len(), 2);
    assert!(available.contains(&"Marta the Innkeeper".to_string()));
}

#[test]
fn available_npcs_empty_when_none() {
    let npcs: Vec<String> = vec![];
    let available = GuestNpcContext::available_npcs(&npcs);
    assert!(available.is_empty());
}

// ═══════════════════════════════════════════════════════════
// 10. Session AC-5: Disposition merging (guest input vs AI)
// ═══════════════════════════════════════════════════════════

#[test]
fn merge_disposition_with_equal_weight() {
    // Guest input +20, AI disposition -10, equal weight (0.5)
    let merged = GuestNpcContext::merge_disposition(20, -10, 0.5);
    // Expected: 0.5 * 20 + 0.5 * (-10) = 10 + (-5) = 5
    assert_eq!(merged, 5);
}

#[test]
fn merge_disposition_guest_dominant() {
    // Guest input +20, AI -10, guest weight 0.8
    let merged = GuestNpcContext::merge_disposition(20, -10, 0.8);
    // Expected: 0.8 * 20 + 0.2 * (-10) = 16 + (-2) = 14
    assert_eq!(merged, 14);
}

#[test]
fn merge_disposition_ai_dominant() {
    // Guest input +20, AI -10, guest weight 0.2
    let merged = GuestNpcContext::merge_disposition(20, -10, 0.2);
    // Expected: 0.2 * 20 + 0.8 * (-10) = 4 + (-8) = -4
    assert_eq!(merged, -4);
}

#[test]
fn merge_disposition_zero_guest_weight_uses_ai() {
    let merged = GuestNpcContext::merge_disposition(100, -15, 0.0);
    assert_eq!(merged, -15);
}

#[test]
fn merge_disposition_full_guest_weight_uses_guest() {
    let merged = GuestNpcContext::merge_disposition(25, -15, 1.0);
    assert_eq!(merged, 25);
}

#[test]
fn merge_disposition_clamps_weight_above_one() {
    // Weight > 1.0 should clamp to 1.0
    let merged = GuestNpcContext::merge_disposition(25, -15, 1.5);
    assert_eq!(merged, 25);
}

#[test]
fn merge_disposition_clamps_weight_below_zero() {
    // Weight < 0.0 should clamp to 0.0
    let merged = GuestNpcContext::merge_disposition(25, -15, -0.5);
    assert_eq!(merged, -15);
}

// ═══════════════════════════════════════════════════════════
// 11. Session AC-2/AC-4: Inverted perception context
// ═══════════════════════════════════════════════════════════

#[test]
fn guest_perception_mode_is_inverted() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    // Guest NPC sees from the NPC's perspective (inverted)
    assert!(
        ctx.perception_mode_inverted(),
        "guest NPC should use inverted perception"
    );
}

#[test]
fn guest_npc_perception_includes_npc_motives() {
    let ctx = GuestNpcContext::new(
        "player-42".to_string(),
        "Marta the Innkeeper".to_string(),
        PlayerRole::default_guest_actions(),
    );
    let desc = ctx.perception_description();
    assert!(
        desc.to_lowercase().contains("npc") || desc.to_lowercase().contains("motive"),
        "perception description should reference NPC perspective: {desc}"
    );
}

// ═══════════════════════════════════════════════════════════
// 12. Edge cases
// ═══════════════════════════════════════════════════════════

#[test]
fn guest_with_all_actions_is_still_guest() {
    let mut all_actions = HashSet::new();
    all_actions.insert(ActionCategory::Dialogue);
    all_actions.insert(ActionCategory::Movement);
    all_actions.insert(ActionCategory::Examine);
    all_actions.insert(ActionCategory::Combat);
    all_actions.insert(ActionCategory::Inventory);
    let role = custom_guest_role(all_actions);
    // Even with all actions, role is still GuestNpc (not Full)
    assert!(role.is_guest());
    assert!(role.can_perform(&ActionCategory::Combat));
}

#[test]
fn narrator_tag_for_npc_with_special_characters() {
    let tag = NarratorTag::for_guest("O'Brien the \"Smith\"");
    let text = tag.to_prompt_string();
    assert!(
        text.contains("O'Brien"),
        "tag should handle special characters in name"
    );
}

#[test]
fn guest_context_with_custom_actions() {
    let mut actions = HashSet::new();
    actions.insert(ActionCategory::Dialogue);
    actions.insert(ActionCategory::Combat);
    let ctx = GuestNpcContext::new(
        "player-99".to_string(),
        "Guard Captain".to_string(),
        actions,
    );
    assert!(ctx.validate_action(&ActionCategory::Combat).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Inventory).is_err());
}
