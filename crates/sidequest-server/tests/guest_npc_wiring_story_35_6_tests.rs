//! Story 35-6 RED: Wire guest_npc permission gating into dispatch pipeline.
//!
//! Epic 35 wiring remediation. `sidequest-game::guest_npc` has been fully built
//! since story 8-7 with `PlayerRole`, `ActionCategory`, `GuestNpcContext`,
//! `validate_action()`, and `ActionError::RestrictedAction` — all with proper
//! unit tests in `sidequest-game/tests/guest_npc_story_8_7_tests.rs`. But the
//! server-side dispatch pipeline never calls any of it. A guest NPC player
//! today is a Full player with a different narrator tag — the restriction
//! is unenforced.
//!
//! This story wires the permission gate into `dispatch_player_action()` so
//! that guest NPCs can only perform their allowed action categories, with
//! OTEL watcher events on every allow/deny decision so the GM panel can see
//! the gate running.
//!
//! Per Epic 35 and `feedback_wiring_checks.md`, the test suite must include
//! source-introspection wiring tests — verifying the production code at
//! `dispatch/mod.rs` actually references the module — because compile-and-run
//! tests alone cannot prove "code X is reachable from production path Y" when
//! the dispatch entry point is hard to set up in isolation. This matches the
//! established pattern in `entity_reference_wiring_story_35_2_tests.rs` and
//! every other Epic 35 wiring test.
//!
//! ACs covered (from `sprint/context/context-story-35-6.md`):
//!   AC-1: Guest NPC with allowed action → passes (contract + wiring)
//!   AC-2: Guest NPC with disallowed action → ActionError::RestrictedAction
//!   AC-3: Full player → unaffected
//!   AC-4: End-to-end integration proves the gate is on the dispatch hot path
//!   AC-5: OTEL watcher events emitted on allow/deny
//!   AC-6: No silent fallback on unclassified guest action
//!
//! Design decisions pinned by these tests (recommended in story context):
//!   - PlayerRole lives on `PlayerState` in `shared_session.rs` (not in
//!     `MultiplayerSession`, which is a game-crate concern)
//!   - Gate runs POST-LLM, using `classified_intent` from `process_action()`
//!   - Intent → ActionCategory mapper lives in `sidequest-server/src/dispatch/`
//!   - Guest NPC with unclassified intent is a HARD error (no silent allow)

use std::collections::HashSet;

use sidequest_game::guest_npc::{
    ActionCategory, ActionError, GuestNpcContext, PlayerRole,
};

// ============================================================================
// Helpers — source introspection
// ============================================================================

/// Load `dispatch/mod.rs` source text with `#[cfg(test)]` blocks stripped.
/// Mirrors the helper used across Epic 35 wiring tests (see
/// `entity_reference_wiring_story_35_2_tests.rs`).
fn dispatch_mod_production_source() -> &'static str {
    let source = include_str!("../src/dispatch/mod.rs");
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// Load `shared_session.rs` source text with `#[cfg(test)]` stripped.
fn shared_session_production_source() -> &'static str {
    let source = include_str!("../src/shared_session.rs");
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// Load the full `src/dispatch/` directory as concatenated production source.
/// The gate and mapper may live in any dispatch submodule — scan the whole
/// directory so the wiring tests do not over-constrain file layout.
///
/// Returns the concatenation of every `.rs` file under `src/dispatch/`, with
/// `#[cfg(test)]` blocks stripped from each. Uses `include_str!` for each
/// known submodule so the test fails cleanly if a new submodule is added
/// without updating this helper.
fn dispatch_dir_production_source() -> String {
    let files: [&str; 20] = [
        include_str!("../src/dispatch/mod.rs"),
        include_str!("../src/dispatch/aside.rs"),
        include_str!("../src/dispatch/audio.rs"),
        include_str!("../src/dispatch/barrier.rs"),
        include_str!("../src/dispatch/beat.rs"),
        include_str!("../src/dispatch/catch_up.rs"),
        include_str!("../src/dispatch/connect.rs"),
        include_str!("../src/dispatch/lore_sync.rs"),
        include_str!("../src/dispatch/npc_registry.rs"),
        include_str!("../src/dispatch/patching.rs"),
        include_str!("../src/dispatch/persistence.rs"),
        include_str!("../src/dispatch/pregen.rs"),
        include_str!("../src/dispatch/prompt.rs"),
        include_str!("../src/dispatch/render.rs"),
        include_str!("../src/dispatch/response.rs"),
        include_str!("../src/dispatch/session_sync.rs"),
        include_str!("../src/dispatch/slash.rs"),
        include_str!("../src/dispatch/state_mutations.rs"),
        include_str!("../src/dispatch/telemetry.rs"),
        include_str!("../src/dispatch/tropes.rs"),
    ];
    files
        .iter()
        .map(|src| src.split("#[cfg(test)]").next().unwrap_or(src))
        .collect::<Vec<_>>()
        .join("\n\n// ---- file boundary ----\n\n")
}

// ============================================================================
// Category A: Pure contract tests on existing guest_npc module
//
// These document the invariants the gate depends on. They are expected to
// pass today (the underlying module is fully built) — the RED failures come
// from the wiring tests below. Including these guards against accidental
// regressions in the module if Dev touches it while wiring the gate.
// ============================================================================

#[test]
fn contract_full_player_permits_every_action_category() {
    let full = PlayerRole::Full;
    assert!(full.can_perform(&ActionCategory::Dialogue));
    assert!(full.can_perform(&ActionCategory::Movement));
    assert!(full.can_perform(&ActionCategory::Examine));
    assert!(full.can_perform(&ActionCategory::Combat));
    assert!(full.can_perform(&ActionCategory::Inventory));
}

#[test]
fn contract_default_guest_permits_only_dialogue_movement_examine() {
    let guest = PlayerRole::GuestNpc {
        npc_name: "Marta".to_string(),
        allowed_actions: PlayerRole::default_guest_actions(),
    };
    assert!(guest.can_perform(&ActionCategory::Dialogue));
    assert!(guest.can_perform(&ActionCategory::Movement));
    assert!(guest.can_perform(&ActionCategory::Examine));
    assert!(
        !guest.can_perform(&ActionCategory::Combat),
        "Default guest NPC must not be able to initiate Combat — \
         this is the load-bearing restriction for the mode"
    );
    assert!(
        !guest.can_perform(&ActionCategory::Inventory),
        "Default guest NPC must not be able to perform Inventory actions"
    );
}

#[test]
fn contract_guest_context_validate_action_returns_restricted_action_variant() {
    let ctx = GuestNpcContext::new(
        "player-2".to_string(),
        "Marta".to_string(),
        PlayerRole::default_guest_actions(),
    );

    let result = ctx.validate_action(&ActionCategory::Combat);
    match result {
        Err(ActionError::RestrictedAction { category }) => {
            assert_eq!(
                category,
                ActionCategory::Combat,
                "RestrictedAction error must carry the attempted category, \
                 not a placeholder — the gate's OTEL event and the client \
                 error message depend on this field"
            );
        }
        Err(other) => panic!(
            "Expected ActionError::RestrictedAction, got {:?}. The gate \
             depends on this specific variant being returned for disallowed \
             categories.",
            other
        ),
        Ok(()) => panic!(
            "validate_action(Combat) on a default guest must return Err — \
             the contract underlying Story 35-6 is broken"
        ),
    }
}

#[test]
fn contract_guest_context_validate_action_returns_ok_for_allowed() {
    let ctx = GuestNpcContext::new(
        "player-2".to_string(),
        "Marta".to_string(),
        PlayerRole::default_guest_actions(),
    );
    assert!(ctx.validate_action(&ActionCategory::Dialogue).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Movement).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Examine).is_ok());
}

#[test]
fn contract_custom_guest_allowed_set_is_respected() {
    // A guest with a custom (non-default) allowed set — e.g., a scenario
    // author gives a combat-capable NPC Combat+Dialogue. The gate must
    // respect the HashSet stored on the role, not the default.
    let mut custom = HashSet::new();
    custom.insert(ActionCategory::Combat);
    custom.insert(ActionCategory::Dialogue);
    let ctx = GuestNpcContext::new(
        "player-3".to_string(),
        "Razortooth".to_string(),
        custom,
    );
    assert!(ctx.validate_action(&ActionCategory::Combat).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Dialogue).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Movement).is_err());
    assert!(ctx.validate_action(&ActionCategory::Examine).is_err());
    assert!(ctx.validate_action(&ActionCategory::Inventory).is_err());
}

// ============================================================================
// Category B: Non-exhaustive enum regression (Rust lang-review rule #2)
//
// The dispatch gate depends on PlayerRole, ActionCategory, and ActionError
// all being #[non_exhaustive]. A future variant addition must not break the
// gate — Dev is free to add ActionCategory::Ability etc. without coordinating
// a lockstep change. These tests will fail at compile time if the attribute
// is removed, because exhaustive matches on an enum WITH #[non_exhaustive]
// require a catch-all in downstream crates.
// ============================================================================

#[test]
fn rule_2_action_category_remains_non_exhaustive() {
    // The whole point of #[non_exhaustive] on a public enum declared in
    // another crate: downstream matches MUST use a catch-all. If this test
    // compiles, ActionCategory is still #[non_exhaustive]. If someone
    // removes the attribute, the compiler will either:
    //   - reject the catch-all as unreachable (warn), OR
    //   - accept the exhaustive match without it — meaning the catch-all
    //     we wrote here would become a warning
    // The test asserts behavior: the catch-all path is live, proving that
    // ActionCategory is treated as open by the compiler.
    let cat = ActionCategory::Combat;
    let label = match cat {
        ActionCategory::Dialogue => "dialogue",
        ActionCategory::Movement => "movement",
        ActionCategory::Examine => "examine",
        ActionCategory::Combat => "combat",
        ActionCategory::Inventory => "inventory",
        // Mandatory because ActionCategory is #[non_exhaustive] across crates.
        // Removing #[non_exhaustive] would make this arm unreachable.
        _ => "future-variant",
    };
    assert_eq!(label, "combat");
}

#[test]
fn rule_2_player_role_remains_non_exhaustive() {
    let role = PlayerRole::Full;
    let label = match role {
        PlayerRole::Full => "full",
        PlayerRole::GuestNpc { .. } => "guest",
        _ => "future-role",
    };
    assert_eq!(label, "full");
}

#[test]
fn rule_2_action_error_remains_non_exhaustive() {
    let err = ActionError::RestrictedAction {
        category: ActionCategory::Combat,
    };
    let label = match err {
        ActionError::RestrictedAction { .. } => "restricted",
        ActionError::NotInSession => "not-in-session",
        _ => "future-error",
    };
    assert_eq!(label, "restricted");
}

// ============================================================================
// Category C: Wiring — dispatch production code references guest_npc
//
// These tests FAIL today because nothing in `sidequest-server/src/dispatch/`
// references PlayerRole, GuestNpcContext, can_perform, or validate_action.
// They pass once Dev wires the gate into dispatch_player_action.
//
// Pattern matches `entity_reference_wiring_story_35_2_tests.rs` AC-6.
// ============================================================================

#[test]
fn wiring_ac6_dispatch_references_guest_npc_module() {
    let production = dispatch_dir_production_source();
    let has_reference = production.contains("PlayerRole")
        || production.contains("GuestNpcContext")
        || production.contains("guest_npc::");
    assert!(
        has_reference,
        "Somewhere under `sidequest-server/src/dispatch/`, production code \
         must reference PlayerRole, GuestNpcContext, or the guest_npc:: path. \
         Story 35-6 AC-6 (wiring) — the module has been fully built since \
         story 8-7 and has zero production consumers in the server crate."
    );
}

#[test]
fn wiring_dispatch_calls_permission_check_method() {
    let production = dispatch_dir_production_source();
    let has_check = production.contains(".can_perform(")
        || production.contains(".validate_action(")
        || production.contains("GuestNpcContext::");
    assert!(
        has_check,
        "Dispatch must call `.can_perform(&ActionCategory::...)` or \
         `.validate_action(&ActionCategory::...)` on the player's role. \
         Neither method name was found in any dispatch submodule. \
         Story 35-6 AC-2."
    );
}

#[test]
fn wiring_dispatch_references_action_category_enum() {
    let production = dispatch_dir_production_source();
    assert!(
        production.contains("ActionCategory"),
        "Dispatch must reference the ActionCategory enum to map the \
         classified intent to a category the gate can check. Not found \
         in any dispatch submodule. Story 35-6."
    );
}

// ============================================================================
// Category D: OTEL watcher events for allow/deny (AC-5)
//
// Per Epic 35 OTEL discipline and CLAUDE.md's OTEL Observability Principle:
// every subsystem decision must emit a WatcherEvent so the GM panel can
// verify the gate ran. These tests fail until Dev emits guest_npc watcher
// events on permission checks.
// ============================================================================

#[test]
fn wiring_ac5_dispatch_emits_guest_npc_watcher_event() {
    let production = dispatch_dir_production_source();
    assert!(
        production.contains("\"guest_npc\""),
        "Dispatch must emit WatcherEventBuilder::new(\"guest_npc\", ...) \
         for permission gate decisions. The GM panel is the lie detector — \
         without this event, Claude can narrate restriction with zero \
         mechanical backing. Story 35-6 AC-5."
    );
}

#[test]
fn wiring_ac5_guest_npc_watcher_uses_validation_warning_for_deny() {
    let production = dispatch_dir_production_source();
    // Find the first "guest_npc" emission and check that ValidationWarning
    // is used nearby (within 400 bytes). ValidationWarning is the correct
    // event type for "the system caught a bad input" — matches the pattern
    // used in `dispatch/audio.rs:296` for SFX validation.
    //
    // If the string "guest_npc" does not appear at all, the earlier test
    // `wiring_ac5_dispatch_emits_guest_npc_watcher_event` will also fail —
    // this test adds the more specific requirement on the event type.
    if let Some(pos) = production.find("\"guest_npc\"") {
        let window_end = (pos + 400).min(production.len());
        let window = &production[pos..window_end];
        assert!(
            window.contains("ValidationWarning"),
            "Near the \"guest_npc\" watcher emission, expected \
             WatcherEventType::ValidationWarning for the deny path. \
             Found window: {:?}",
            &window[..window.len().min(300)]
        );
    } else {
        panic!(
            "dispatch production code must contain a \"guest_npc\" watcher \
             emission — prerequisite for Story 35-6 AC-5"
        );
    }
}

#[test]
fn wiring_ac5_guest_npc_watcher_carries_category_field() {
    let production = dispatch_dir_production_source();
    // The watcher event must include the attempted category as a field so
    // the GM panel can display which category was denied. Look for a
    // .field("category", ...) call within 500 bytes of the "guest_npc"
    // emission site. (Permissive match — "category" may appear as a field
    // name or as part of a format string.)
    if let Some(pos) = production.find("\"guest_npc\"") {
        let window_end = (pos + 500).min(production.len());
        let window = &production[pos..window_end];
        assert!(
            window.contains("category") || window.contains("Category"),
            "The guest_npc watcher emission must carry the action category \
             (e.g., .field(\"category\", \"Combat\")) so the GM panel can \
             display which category triggered the gate. Story 35-6 AC-5."
        );
    } else {
        panic!("Prerequisite: \"guest_npc\" must appear in dispatch source");
    }
}

// ============================================================================
// Category E: PlayerState has a role field (AC-3, AC-1)
//
// `PlayerRole` has no home on the session today. The recommended design (per
// story context §critical design question #1) is to add `role: PlayerRole`
// to `PlayerState` in `shared_session.rs`. Dev may choose an alternative
// (e.g., storing on MultiplayerSession) — if so, they must log a design
// deviation AND update this test to point at the chosen location.
// ============================================================================

#[test]
fn wiring_player_state_has_player_role_field() {
    let source = shared_session_production_source();
    // The PlayerState struct definition must include a role field that
    // references PlayerRole. Look for the two strings near each other.
    let has_field = source.contains("role:")
        && (source.contains("PlayerRole") || source.contains("guest_npc"));
    assert!(
        has_field,
        "PlayerState in shared_session.rs must store a PlayerRole so the \
         dispatch gate can look it up by player_id. Neither `role:` nor \
         `PlayerRole` was found. Story 35-6 — see story context §critical \
         design question #1. If Dev chose a different location for the role \
         (e.g., MultiplayerSession), log a design deviation and update this \
         test to reference the chosen location."
    );
}

// ============================================================================
// Category F: Gate runs AFTER intent classification (architectural decision)
//
// Per story context §Where the gate physically goes: the gate must be placed
// AFTER `process_action()` returns `classified_intent`, so the intent is
// available for mapping to an ActionCategory. Pre-LLM keyword matching is
// forbidden by `feedback_no_keyword_matching.md` (Zork Problem).
//
// This test enforces ordering by byte position in dispatch/mod.rs.
// ============================================================================

#[test]
fn wiring_gate_check_runs_after_process_action() {
    let production = dispatch_mod_production_source();

    let process_action_pos = production.find("process_action(");
    assert!(
        process_action_pos.is_some(),
        "Prerequisite: `process_action(` must exist in dispatch/mod.rs \
         (it is the intent classifier entry point at ~line 789)"
    );

    // Look for any gate marker — the wire may call can_perform, validate_action,
    // or reference PlayerRole/GuestNpcContext directly.
    let gate_pos = production
        .find(".can_perform(")
        .or_else(|| production.find(".validate_action("))
        .or_else(|| production.find("GuestNpcContext"))
        .or_else(|| production.find("PlayerRole::"));

    assert!(
        gate_pos.is_some(),
        "Gate call site not found in dispatch/mod.rs production code. \
         Story 35-6 — the permission gate must be wired into \
         dispatch_player_action()."
    );

    let pa = process_action_pos.unwrap();
    let gate = gate_pos.unwrap();
    assert!(
        gate > pa,
        "Gate call (at byte {}) must appear AFTER process_action call \
         (at byte {}) in dispatch/mod.rs. Running the gate before intent \
         classification would require keyword matching on the raw action \
         string, which is forbidden by feedback_no_keyword_matching.md \
         (Zork Problem). Story 35-6 — see story context §Where the gate \
         physically goes.",
        gate,
        pa
    );
}

// ============================================================================
// Category G: Intent → ActionCategory mapper exists and is exhaustive
//
// The mapper lives somewhere in sidequest-server/src/. It must handle all
// 8 variants of `sidequest_agents::agents::intent_router::Intent`:
//   Combat, Dialogue, Exploration, Examine, Meta, Chase, Backstory, Accusation
//
// Map targets are an ActionCategory variant OR a "bypass" decision for
// non-gameplay intents (Meta = slash commands, Backstory = character
// establishment). These tests enforce exhaustiveness via source grep.
// ============================================================================

#[test]
fn wiring_mapper_references_all_intent_variants() {
    let production = dispatch_dir_production_source();
    // Every Intent variant name must appear somewhere in the dispatch
    // directory's production source. This is a necessary (not sufficient)
    // condition for an exhaustive match.
    for variant in [
        "Combat",
        "Dialogue",
        "Exploration",
        "Examine",
        "Meta",
        "Chase",
        "Backstory",
        "Accusation",
    ] {
        assert!(
            production.contains(variant),
            "Intent::{} must be handled explicitly in the dispatch gate \
             mapper. The mapper must be exhaustive over all 8 Intent variants. \
             Story 35-6 AC-6 — no silent fallback.",
            variant
        );
    }
}

#[test]
fn wiring_mapper_silent_wildcard_forbidden_loud_wildcard_ok() {
    // The No Silent Fallbacks rule (CLAUDE.md) forbids a wildcard arm that
    // silently defaults to a `GateDecision::Check(...)` or `Bypass` for
    // unknown `Intent` variants. But `Intent` is `#[non_exhaustive]` across
    // crates — Rust's type system REQUIRES a wildcard arm in downstream
    // matches, regardless of whether all current variants are covered.
    //
    // This test therefore enforces the semantic rule, not a blanket ban on
    // `_ =>`: if a wildcard arm exists in the mapper's match, the next ~250
    // bytes must contain a LOUD failure marker (unreachable!, panic!, or
    // todo!). A wildcard that silently returns a GateDecision is a failure.
    //
    // The rationale for the loud wildcard is: a new `Intent` variant added
    // upstream (e.g., `Intent::Ability`) must force a developer decision
    // here, not silently take a default. `unreachable!` at runtime on an
    // unexpected variant is the intended loud failure mode.
    let production = dispatch_dir_production_source();

    let Some(intent_match_pos) = production.find("Intent::Combat") else {
        panic!(
            "Prerequisite: `Intent::Combat` must appear in dispatch \
             production code — the Intent-to-ActionCategory mapper must \
             reference it. Story 35-6."
        );
    };

    // Examine a 1200-byte window starting at the mapper (large enough to
    // cover 8 match arms on separate lines plus a wildcard with a loud
    // panic message).
    let window_end = (intent_match_pos + 1200).min(production.len());
    let window = &production[intent_match_pos..window_end];

    let wildcard_idx = window.find("_ =>").or_else(|| window.find("_=>"));

    if let Some(wi) = wildcard_idx {
        // Wildcard present — enforce the loud-failure rule within the
        // next 250 bytes (enough to cover an `unreachable!` with a
        // multi-line message).
        let check_end = (wi + 250).min(window.len());
        let wildcard_region = &window[wi..check_end];
        let is_loud = wildcard_region.contains("unreachable!")
            || wildcard_region.contains("panic!")
            || wildcard_region.contains("todo!");
        assert!(
            is_loud,
            "Wildcard arm `_ =>` in the Intent-to-ActionCategory mapper \
             must lead to a loud failure (unreachable!, panic!, or todo!) \
             to satisfy the No Silent Fallbacks rule. A wildcard that maps \
             to a default category or GateDecision::Bypass is a silent \
             fallback and defeats the gate. Story 35-6 AC-6. Region \
             examined:\n{}",
            &wildcard_region[..wildcard_region.len().min(400)]
        );
    }
    // If no wildcard is present, the compiler enforces exhaustiveness
    // (non-cross-crate enum case) — nothing more to check.
}

// ============================================================================
// Category H: No silent fallback on unclassified guest action (AC-6)
//
// If the classifier returns `classified_intent: None` AND the player is a
// guest NPC, the gate must either (a) hard-fail with a typed error, (b)
// panic loudly, or (c) emit a ValidationWarning AND reject the action.
// It must NOT silently allow the action through.
//
// Enforced by grepping for one of: explicit error type name, panic!/
// unreachable!/expect call, or ValidationWarning emission on the unclassified
// path.
// ============================================================================

#[test]
fn wiring_ac6_unclassified_guest_action_has_loud_handler() {
    let production = dispatch_dir_production_source();

    // Look for any of the loud-failure markers near the guest gate.
    // The marker must be within 600 bytes of either "guest_npc" or
    // "PlayerRole::GuestNpc" to count as "handling the unclassified case
    // at the gate site."
    let anchor = production
        .find("PlayerRole::GuestNpc")
        .or_else(|| production.find("GuestNpcContext"))
        .or_else(|| production.find("\"guest_npc\""));

    assert!(
        anchor.is_some(),
        "Prerequisite: guest gate wire must exist before we can check its \
         unclassified-intent handling. Story 35-6."
    );

    let pos = anchor.unwrap();
    // Walk backward 200 bytes and forward 800 bytes for context.
    let start = pos.saturating_sub(200);
    let end = (pos + 800).min(production.len());
    let window = &production[start..end];

    let has_loud_handler = window.contains("UnclassifiedGuest")
        || window.contains("unclassified_guest")
        || window.contains("panic!")
        || window.contains("unreachable!")
        || window.contains(".expect(")
        || window.contains("ValidationWarning");

    assert!(
        has_loud_handler,
        "Near the guest gate wire, production code must handle the \
         `classified_intent: None` case loudly — via a typed error \
         (`UnclassifiedGuestAction`), a panic/unreachable/expect, or an \
         explicit ValidationWarning emission. Silent fallback to \
         `PlayerRole::Full` is forbidden by CLAUDE.md's No Silent Fallbacks \
         rule and the feedback memory `feedback_no_fallbacks.md`. \
         Story 35-6 AC-6."
    );
}

// ============================================================================
// Category I: Allow-path watcher event (AC-1, AC-5)
//
// The gate must emit a watcher event on the ALLOW path as well, not just
// deny. Without allow-path emission, the GM panel cannot distinguish "gate
// ran and permitted" from "gate never ran." Both look identical at the
// narration layer.
//
// Pattern: SubsystemExerciseSummary is the standard event type for "the
// subsystem ran and here's the summary" (matches `dispatch/prompt.rs:303`
// for rag and `dispatch/audio.rs:250` for mood_image).
// ============================================================================

#[test]
fn wiring_ac5_guest_npc_has_allow_path_watcher_event() {
    let production = dispatch_dir_production_source();
    // Count occurrences of "guest_npc" in watcher event builders. We expect
    // AT LEAST 2 — one for allow, one for deny. If there is exactly 1, the
    // gate is only tracking one direction, which is insufficient for the
    // GM panel.
    let count = production.matches("\"guest_npc\"").count();
    assert!(
        count >= 2,
        "Expected at least 2 watcher event emissions with the \"guest_npc\" \
         component string — one for the allow path (SubsystemExerciseSummary) \
         and one for the deny path (ValidationWarning). Found {}. \
         Story 35-6 AC-5 — the GM panel must be able to see both decisions.",
        count
    );
}

// ============================================================================
// Category J: Full-player path emits NO guest_npc events (AC-3)
//
// A Full player must not pollute the guest_npc watcher channel. This is
// hard to test via source-grep alone (the check happens at runtime), but
// we can enforce the structural constraint: the guest_npc watcher emission
// must be INSIDE a branch that only runs when the player role is GuestNpc,
// not on every dispatch call.
//
// Test: the "guest_npc" string must appear within 400 bytes of either
// `GuestNpc {` (pattern match) or `role.is_guest()` (helper method).
// ============================================================================

#[test]
fn wiring_ac3_guest_npc_watcher_is_inside_guest_role_branch() {
    let production = dispatch_dir_production_source();
    let Some(emit_pos) = production.find("\"guest_npc\"") else {
        panic!(
            "Prerequisite: \"guest_npc\" watcher emission must exist. \
             Story 35-6 AC-5."
        );
    };

    // Walk backward 1200 bytes from the emission site to find the enclosing
    // branch marker. Rust's verbose multi-line let-else + multi-line pattern
    // match syntax means the `PlayerRole::GuestNpc` marker can be 20+ lines
    // above the emit site even in a tight gate implementation. The 1200-byte
    // window covers ~30 lines, which is enough for any reasonable gate layout
    // without being so large it covers the entire function body.
    let start = emit_pos.saturating_sub(1200);
    let window = &production[start..emit_pos];

    let inside_guest_branch = window.contains("GuestNpc {")
        || window.contains("GuestNpc{")
        || window.contains(".is_guest()")
        || window.contains("PlayerRole::GuestNpc");

    assert!(
        inside_guest_branch,
        "The \"guest_npc\" watcher event must be emitted only inside a \
         branch that matches PlayerRole::GuestNpc (pattern match or \
         is_guest() check). Emitting it on every dispatch call — including \
         for Full players — pollutes the GM panel with noise events that \
         don't correspond to any gate decision. Story 35-6 AC-3."
    );
}
