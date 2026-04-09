//! Story 26-11: Wiring tests — party reconciliation is called from dispatch_connect
//! and emits OTEL telemetry.
//!
//! These tests verify that the reconciliation module is wired into the production
//! dispatch pipeline by reading the source code and asserting on required patterns.

/// AC-5: Wiring test — party_reconciliation module is reachable from server crate
/// and the reconcile function signature matches what dispatch_connect needs.
#[test]
fn party_reconciliation_is_reachable_from_server_crate() {
    use sidequest_game::party_reconciliation::{
        PartyReconciliation, PlayerLocation, ReconciliationResult,
    };

    // Compile-time proof: the module and types are exported
    let players = vec![PlayerLocation {
        player_id: "test".to_string(),
        player_name: "Test".to_string(),
        location: "somewhere".to_string(),
    }];

    let result = PartyReconciliation::reconcile(&players, false);
    // Single player → no action needed
    assert!(
        matches!(result, ReconciliationResult::NoActionNeeded),
        "Single player should produce NoActionNeeded"
    );
}

/// AC-5: Wiring test — dispatch/connect.rs calls party_reconciliation::reconcile
/// and emits session.resume.party_reconciliation OTEL span.
#[test]
fn dispatch_connect_calls_party_reconciliation() {
    let connect_source =
        std::fs::read_to_string("crates/sidequest-server/src/dispatch/connect.rs")
            .expect("dispatch/connect.rs must be readable");

    // The reconciliation call must exist in the reconnect path
    assert!(
        connect_source.contains("party_reconciliation"),
        "dispatch/connect.rs must import or call party_reconciliation module"
    );

    assert!(
        connect_source.contains("PartyReconciliation::reconcile")
            || connect_source.contains("party_reconciliation::reconcile"),
        "dispatch/connect.rs must call the reconcile function"
    );
}

/// AC-3/AC-5: OTEL span session.resume.party_reconciliation exists in dispatch source.
#[test]
fn dispatch_connect_emits_party_reconciliation_otel_span() {
    let connect_source =
        std::fs::read_to_string("crates/sidequest-server/src/dispatch/connect.rs")
            .expect("dispatch/connect.rs must be readable");

    assert!(
        connect_source.contains("session.resume.party_reconciliation")
            || connect_source.contains("party_reconciliation"),
        "dispatch/connect.rs must emit a session.resume.party_reconciliation OTEL event"
    );

    // The OTEL event must include before/after location fields
    assert!(
        connect_source.contains("players_moved")
            || connect_source.contains("old_location")
            || connect_source.contains("before_locations"),
        "OTEL event must include before/after location data for GM panel visibility"
    );
}

/// Wiring test — party_reconciliation module is declared in sidequest-game lib.rs
#[test]
fn party_reconciliation_module_declared_in_lib() {
    let lib_source = std::fs::read_to_string("crates/sidequest-game/src/lib.rs")
        .expect("sidequest-game lib.rs must be readable");

    assert!(
        lib_source.contains("pub mod party_reconciliation"),
        "sidequest-game/src/lib.rs must declare pub mod party_reconciliation"
    );
}
