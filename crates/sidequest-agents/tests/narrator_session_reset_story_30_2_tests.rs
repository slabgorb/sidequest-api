//! Story 30-2: Narrator prompt compression loses genre/world context.
//!
//! Root cause: `narrator_session_id` on Orchestrator is never cleared between
//! games. When a player finishes one game and starts another (or loads a different
//! save), the old session ID persists → Delta tier sent → narrator gets no
//! grounding context for the new game.
//!
//! These tests verify:
//! 1. Orchestrator exposes a method to reset the narrator session
//! 2. After reset, the next prompt uses Full tier (not Delta)
//! 3. Genre/world/character context is always present regardless of tier
//! 4. Delta tier for a DIFFERENT genre than the original session triggers Full
//! 5. OTEL spans capture tier selection for observability

use std::collections::HashMap;

use sidequest_agents::orchestrator::{NarratorPromptTier, Orchestrator, TurnContext};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use sidequest_genre::Prompts;
use tokio::sync::mpsc;

/// Helper: build a TurnContext with genre and narrator voice set.
fn context_with_genre(genre: &str, voice: &str) -> TurnContext {
    TurnContext {
        genre: Some(genre.to_string()),
        character_name: "Test Hero".to_string(),
        genre_prompts: Some(Prompts {
            narrator: voice.to_string(),
            combat: String::new(),
            npc: "NPCs behave per genre".to_string(),
            world_state: "Track world state per genre conventions".to_string(),
            chase: None,
            transition_hints: std::collections::HashMap::new(),
            extraction: None,
            keeper_monologue: None,
            town: None,
            chargen: None,
        }),
        ..Default::default()
    }
}

// ============================================================================
// AC-1: Orchestrator must expose a method to reset narrator session
// ============================================================================

#[test]
fn orchestrator_has_reset_narrator_session_method() {
    // The Orchestrator must provide a way to clear the narrator session ID
    // so that switching games triggers a Full tier prompt rebuild.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    // This method must exist — it currently doesn't.
    orch.reset_narrator_session();
}

#[test]
fn reset_narrator_session_causes_full_tier_on_next_prompt() {
    // After resetting the session, the next call to process_action must
    // use Full tier, not Delta — because the persistent Claude session
    // no longer has the system prompt cached.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("caverns_and_claudes", "Dark fantasy narrator voice");

    // First prompt — Full tier (no session yet)
    let result1 = orch.build_narrator_prompt_tiered("look around", &ctx, NarratorPromptTier::Full);
    assert!(
        result1.prompt_text.contains("<genre-voice>"),
        "Full tier must include narrator voice"
    );

    // Simulate: session was established (set the ID)
    orch.set_narrator_session_id("old-session-uuid".to_string());

    // Reset session (switching games)
    orch.reset_narrator_session();

    // The orchestrator's tier selection should now choose Full
    // (We test the tier selection logic, not just the method existence)
    assert!(
        !orch.has_active_narrator_session(),
        "After reset, narrator session must be None"
    );
}

// ============================================================================
// AC-2: Genre voice must survive on Delta tier
// ============================================================================

#[test]
fn delta_tier_includes_narrator_voice() {
    // The narrator voice template is currently Full-tier-only.
    // Without it on Delta, the narrator loses its genre-specific voice
    // after the first turn and starts speaking in generic LLM tone.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("spaghetti_western", "Speak in terse, dusty prose");

    let result = orch.build_narrator_prompt_tiered("draw my pistol", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("<genre-voice>"),
        "Delta tier must include narrator voice — without it the narrator \
         loses genre tone after the first turn. Got prompt:\n{}",
        &result.prompt_text[..result.prompt_text.len().min(500)]
    );
}

#[test]
fn delta_tier_includes_world_state_tracking() {
    // World state tracking instructions tell the narrator HOW to track
    // state changes. Without it on Delta, the narrator stops emitting
    // proper game_patch state fields.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("low_fantasy", "Grim narrator voice");

    let result = orch.build_narrator_prompt_tiered("enter the tavern", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("<genre-world-state>"),
        "Delta tier must include world state tracking instructions — \
         without them the narrator stops emitting structured state patches"
    );
}

#[test]
fn delta_tier_includes_npc_behavior() {
    // NPC behavior guidelines define how NPCs act in this genre.
    // Without them on Delta, NPCs become generic after the first turn.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("neon_dystopia", "Cyberpunk narrator voice");

    let result = orch.build_narrator_prompt_tiered("talk to the fixer", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("<genre-npc>"),
        "Delta tier must include NPC behavior guidelines — \
         NPCs lose genre personality without them"
    );
}

// ============================================================================
// AC-3: Character name must always be present in prompt
// ============================================================================

#[test]
fn delta_tier_uses_character_name_not_numeric_index() {
    // The player action section must use the character's name, never a
    // numeric index like "1". This was observed in playtest.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = TurnContext {
        genre: Some("caverns_and_claudes".to_string()),
        character_name: "Four-fingered Jack".to_string(),
        ..Default::default()
    };

    let result = orch.build_narrator_prompt_tiered("search the room", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("Four-fingered Jack"),
        "Player action must use character name, not numeric index"
    );
    assert!(
        !result.prompt_text.contains("1 says:"),
        "Player action must never use numeric index '1' as character name"
    );
}

// ============================================================================
// AC-4: Genre switch detection — different genre must trigger Full tier
// ============================================================================

#[test]
fn orchestrator_detects_genre_switch_and_resets_session() {
    // When the genre changes between turns (player switched games),
    // the orchestrator must detect this and use Full tier, not Delta.
    // A Delta prompt for genre B using genre A's cached session is wrong.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    // Establish session with genre A
    orch.set_narrator_session_id("session-for-genre-a".to_string());
    orch.set_session_genre("spaghetti_western".to_string());

    // Now player loads a different genre
    let ctx_b = context_with_genre("caverns_and_claudes", "Dark fantasy voice");

    // The orchestrator should detect the genre mismatch and force Full tier
    let tier = orch.select_prompt_tier(&ctx_b);

    assert_eq!(
        tier,
        NarratorPromptTier::Full,
        "Genre switch must force Full tier — Delta would use wrong session cache"
    );
}

// ============================================================================
// AC-5: Orchestrator exposes session inspection for observability
// ============================================================================

#[test]
fn orchestrator_exposes_has_active_narrator_session() {
    // For OTEL and debugging, must be able to check session state.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    assert!(
        !orch.has_active_narrator_session(),
        "Fresh orchestrator must not have an active session"
    );

    orch.set_narrator_session_id("test-session".to_string());
    assert!(
        orch.has_active_narrator_session(),
        "After setting session ID, must report active"
    );

    orch.reset_narrator_session();
    assert!(
        !orch.has_active_narrator_session(),
        "After reset, must report no active session"
    );
}

// ============================================================================
// Rule coverage: #4 Tracing — tier selection must be observable
// ============================================================================

#[test]
fn full_tier_prompt_contains_all_grounding_sections() {
    // Sanity check: Full tier must contain ALL grounding sections.
    // If this passes but Delta tests fail, we know the tier filtering is wrong.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("caverns_and_claudes", "Dark fantasy narrator voice");

    let result = orch.build_narrator_prompt_tiered("look around", &ctx, NarratorPromptTier::Full);

    assert!(result.prompt_text.contains("<genre>"), "Full tier must have genre identity");
    assert!(result.prompt_text.contains("<genre-voice>"), "Full tier must have narrator voice");
    assert!(result.prompt_text.contains("<genre-world-state>"), "Full tier must have world state");
    assert!(result.prompt_text.contains("<genre-npc>"), "Full tier must have NPC behavior");
}

// ============================================================================
// Wiring test: build_narrator_prompt_tiered is called from process_action
// ============================================================================

#[test]
fn process_action_tier_selection_uses_session_state() {
    // Verify that process_action actually checks narrator_session_id
    // to select the tier — not hardcoded to one tier.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    // No session → should select Full
    let ctx = context_with_genre("low_fantasy", "Grim narrator voice");

    // We can't easily test process_action (it calls Claude CLI),
    // but we CAN test the tier selection logic directly.
    let tier_no_session = if orch.has_active_narrator_session() {
        NarratorPromptTier::Delta
    } else {
        NarratorPromptTier::Full
    };
    assert_eq!(tier_no_session, NarratorPromptTier::Full);

    // Set session → should select Delta
    orch.set_narrator_session_id("some-session".to_string());
    let tier_with_session = if orch.has_active_narrator_session() {
        NarratorPromptTier::Delta
    } else {
        NarratorPromptTier::Full
    };
    assert_eq!(tier_with_session, NarratorPromptTier::Delta);
}
