//! Story 37-7: Chargen back button wiring verification.
//!
//! The UI sends CHARACTER_CREATION with `action: "back"` when the player
//! clicks the back button during character generation. The server must:
//!   1. Accept the `action` field in the payload (serde won't reject it)
//!   2. Route `action: "back"` to a handler that steps the builder backward
//!   3. Emit OTEL telemetry for the back-navigation event
//!
//! These are source-inspection wiring tests following the project convention
//! (see test_helpers.rs). They verify the structural presence of handling
//! code so a future refactor can't silently remove it.

use crate::test_helpers;

// =========================================================================
// Protocol layer — CharacterCreationPayload must have an `action` field
// =========================================================================

/// The struct must declare an `action` field so `deny_unknown_fields` doesn't
/// reject the UI's `{ action: "back" }` payload.
#[test]
fn chargen_payload_has_action_field() {
    let protocol_src = include_str!("../../../sidequest-protocol/src/message.rs");

    // Look for the action field declaration within CharacterCreationPayload.
    // Must be an Optional<String> or Option<String> to remain backwards-compatible.
    let in_payload = protocol_src
        .find("pub struct CharacterCreationPayload")
        .expect("CharacterCreationPayload struct must exist");

    // Find the closing brace of the struct (next `}` after the struct start)
    let struct_body = &protocol_src[in_payload..];
    let struct_end = struct_body
        .find("\n}")
        .expect("struct must have closing brace");
    let struct_text = &struct_body[..struct_end];

    assert!(
        struct_text.contains("action") && struct_text.contains("Option<String>"),
        "CharacterCreationPayload must have an `action: Option<String>` field.\n\
         Without it, #[serde(deny_unknown_fields)] rejects the UI's back button payload.\n\
         Struct body:\n{}",
        struct_text
    );
}

/// The struct must also declare a `target_step` field for edit-from-review
/// navigation (the UI sends `{ action: "edit", targetStep: N }`).
#[test]
fn chargen_payload_has_target_step_field() {
    let protocol_src = include_str!("../../../sidequest-protocol/src/message.rs");

    let in_payload = protocol_src
        .find("pub struct CharacterCreationPayload")
        .expect("CharacterCreationPayload struct must exist");

    let struct_body = &protocol_src[in_payload..];
    let struct_end = struct_body
        .find("\n}")
        .expect("struct must have closing brace");
    let struct_text = &struct_body[..struct_end];

    assert!(
        struct_text.contains("target_step"),
        "CharacterCreationPayload must have a `target_step` field for edit navigation.\n\
         The UI sends {{ action: \"edit\", targetStep: N }} from the review screen.\n\
         Struct body:\n{}",
        struct_text
    );
}

// =========================================================================
// Dispatch layer — connect.rs must handle the back action
// =========================================================================

/// The dispatch function must check for `action: "back"` and route it
/// to a handler that steps the CharacterBuilder backward.
#[test]
fn dispatch_handles_chargen_back_action() {
    let dispatch_src = test_helpers::dispatch_source_combined();

    // The dispatch must check for the "back" action value somewhere in
    // the chargen dispatch path. This could be a match arm, an if-let,
    // or a method call — but the string "back" must appear in the context
    // of action handling within dispatch_character_creation.
    assert!(
        dispatch_src.contains(r#"action"#)
            && dispatch_src.contains("back")
            && dispatch_src.contains("dispatch_character_creation"),
        "dispatch_character_creation must handle action:back.\n\
         The UI sends {{ action: \"back\" }} but the server has no handler for it.\n\
         The back button silently does nothing because the server ignores the action field."
    );
}

/// The dispatch must also handle `action: "edit"` for review-screen navigation.
#[test]
fn dispatch_handles_chargen_edit_action() {
    let dispatch_src = test_helpers::dispatch_source_combined();

    assert!(
        dispatch_src.contains(r#""edit""#) && dispatch_src.contains("target_step"),
        "dispatch_character_creation must handle action:edit with target_step.\n\
         The UI sends {{ action: \"edit\", targetStep: N }} from the review screen\n\
         to jump back to a specific chargen step."
    );
}

// =========================================================================
// OTEL telemetry — back navigation must emit watcher events
// =========================================================================

/// Back navigation must emit an OTEL watcher event so the GM panel can
/// verify the back button is actually being processed, not silently dropped.
#[test]
fn chargen_back_emits_otel_event() {
    let dispatch_src = test_helpers::dispatch_source_combined();

    // There should be a WatcherEventBuilder call that includes "back" or
    // "navigate_back" in the context of chargen.
    let has_back_telemetry = dispatch_src.contains("WatcherEventBuilder")
        && (dispatch_src.contains(r#""navigate_back""#)
            || dispatch_src.contains(r#""action_back""#)
            || dispatch_src.contains(r#""chargen_back""#)
            || (dispatch_src.contains(r#""back""#) && dispatch_src.contains("character_creation")));

    assert!(
        has_back_telemetry,
        "Chargen back navigation must emit OTEL telemetry via WatcherEventBuilder.\n\
         Without it, the GM panel can't tell if the back button is working or\n\
         if Claude is just improvising. The OTEL principle applies here."
    );
}

// =========================================================================
// CharacterBuilder — must support stepping backward
// =========================================================================

/// The CharacterBuilder must expose a method to step backward (e.g., `go_back()`
/// or `step_back()`). Without it, dispatch has nothing to call.
#[test]
fn character_builder_has_back_method() {
    let builder_src = include_str!("../../../sidequest-game/src/builder.rs");

    let has_back = builder_src.contains("fn go_back")
        || builder_src.contains("fn step_back")
        || builder_src.contains("fn navigate_back")
        || builder_src.contains("fn back");

    assert!(
        has_back,
        "CharacterBuilder must have a method to navigate backward (go_back/step_back/back).\n\
         Without it, dispatch_character_creation has nothing to call when action:back arrives."
    );
}
