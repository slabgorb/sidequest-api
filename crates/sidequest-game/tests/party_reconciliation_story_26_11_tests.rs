//! Story 26-11: Party reconciliation on session resume
//!
//! Tests for the party_reconciliation module which detects and resolves
//! divergent player locations during multiplayer session resume.
//!
//! Bug: On multiplayer reconnect, each player's persisted location is restored
//! independently, causing split-party states (Kael @ antechamber, Mira @ mouth)
//! without narrative justification.
//!
//! The reconciliation module provides:
//! - Detection of divergent locations across a set of player snapshots
//! - Resolution to a canonical "rally point" location
//! - Generation of a reconciliation narration line
//! - OTEL-ready telemetry data (before/after locations per player)
//! - Opt-out via a split_party flag on the scenario/genre

use sidequest_game::party_reconciliation::{
    PartyReconciliation, PlayerLocation, ReconciliationResult,
};

// ---------------------------------------------------------------------------
// AC-1: Divergent locations snap to a single reconciled location
// ---------------------------------------------------------------------------

#[test]
fn divergent_locations_are_reconciled_to_single_location() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "mouth".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref target_location,
            ref players_moved,
            ..
        } => {
            // All players should end up at the same location
            assert!(
                !target_location.is_empty(),
                "Target location must not be empty"
            );
            // At least one player was moved
            assert!(
                !players_moved.is_empty(),
                "At least one player should have been moved"
            );
            // Every moved player's new location matches the target
            for moved in players_moved {
                assert_eq!(
                    moved.new_location, *target_location,
                    "Moved player {} should be at target location",
                    moved.player_name
                );
            }
        }
        other => panic!(
            "Expected Reconciled result for divergent locations, got {:?}",
            other
        ),
    }
}

#[test]
fn same_location_produces_no_reconciliation() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "antechamber".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::NoActionNeeded => {}
        other => panic!(
            "Expected NoActionNeeded when all players at same location, got {:?}",
            other
        ),
    }
}

#[test]
fn single_player_produces_no_reconciliation() {
    let players = vec![PlayerLocation {
        player_id: "p1".to_string(),
        player_name: "Kael".to_string(),
        location: "antechamber".to_string(),
    }];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::NoActionNeeded => {}
        other => panic!(
            "Expected NoActionNeeded for single player, got {:?}",
            other
        ),
    }
}

#[test]
fn empty_players_produces_no_reconciliation() {
    let players: Vec<PlayerLocation> = vec![];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::NoActionNeeded => {}
        other => panic!(
            "Expected NoActionNeeded for empty player list, got {:?}",
            other
        ),
    }
}

#[test]
fn three_players_two_locations_reconciles_to_majority() {
    // Two players at antechamber, one at mouth — majority wins
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "mouth".to_string(),
        },
        PlayerLocation {
            player_id: "p3".to_string(),
            player_name: "Thessa".to_string(),
            location: "antechamber".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref target_location,
            ref players_moved,
            ..
        } => {
            assert_eq!(
                target_location, "antechamber",
                "Majority location (antechamber) should win"
            );
            assert_eq!(
                players_moved.len(),
                1,
                "Only the minority player should be moved"
            );
            assert_eq!(players_moved[0].player_name, "Mira");
            assert_eq!(players_moved[0].old_location, "mouth");
            assert_eq!(players_moved[0].new_location, "antechamber");
        }
        other => panic!("Expected Reconciled, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// AC-2: Reconciliation emits a narration line
// ---------------------------------------------------------------------------

#[test]
fn reconciliation_produces_narration_text() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "mouth".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref narration_text, ..
        } => {
            assert!(
                !narration_text.is_empty(),
                "Reconciliation must produce narration text"
            );
            // The narration should mention the target location
            // (exact wording is implementation detail, but location must appear)
        }
        other => panic!("Expected Reconciled with narration, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// AC-3: OTEL telemetry data (before/after per player)
// ---------------------------------------------------------------------------

#[test]
fn reconciliation_result_contains_telemetry_data() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "mouth".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref players_moved,
            ref target_location,
            ..
        } => {
            // Each moved player entry must have before/after for OTEL emission
            for moved in players_moved {
                assert!(
                    !moved.player_id.is_empty(),
                    "Telemetry needs player_id"
                );
                assert!(
                    !moved.old_location.is_empty(),
                    "Telemetry needs old_location"
                );
                assert!(
                    !moved.new_location.is_empty(),
                    "Telemetry needs new_location"
                );
                assert_ne!(
                    moved.old_location, moved.new_location,
                    "Only actually-moved players should be in the moved list"
                );
            }
            assert!(
                !target_location.is_empty(),
                "Telemetry needs target_location"
            );
        }
        other => panic!("Expected Reconciled with telemetry data, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// AC-4: Split-party flag opt-out
// ---------------------------------------------------------------------------

#[test]
fn split_party_flag_preserves_divergent_locations() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "mouth".to_string(),
        },
    ];

    // split_party_allowed = true → no reconciliation
    let result = PartyReconciliation::reconcile(&players, true);

    match result {
        ReconciliationResult::SplitPartyAllowed => {}
        other => panic!(
            "Expected SplitPartyAllowed when flag is true, got {:?}",
            other
        ),
    }
}

#[test]
fn split_party_flag_with_same_location_still_no_action() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "antechamber".to_string(),
        },
    ];

    // Even with split_party allowed, same location = no action needed
    let result = PartyReconciliation::reconcile(&players, true);

    match result {
        ReconciliationResult::NoActionNeeded => {}
        other => panic!(
            "Expected NoActionNeeded when all at same location (flag irrelevant), got {:?}",
            other
        ),
    }
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn player_with_empty_location_is_always_moved() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: String::new(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref target_location,
            ref players_moved,
            ..
        } => {
            assert_eq!(target_location, "antechamber");
            assert_eq!(players_moved.len(), 1);
            assert_eq!(players_moved[0].player_name, "Mira");
        }
        other => panic!("Expected Reconciled for empty-location player, got {:?}", other),
    }
}

#[test]
fn all_players_empty_location_produces_no_action() {
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: String::new(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: String::new(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::NoActionNeeded => {}
        other => panic!(
            "Expected NoActionNeeded when all locations empty, got {:?}",
            other
        ),
    }
}

#[test]
fn four_players_even_split_picks_first_alphabetically() {
    // 2 at antechamber, 2 at mouth — tie-breaking must be deterministic
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Kael".to_string(),
            location: "mouth".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Mira".to_string(),
            location: "antechamber".to_string(),
        },
        PlayerLocation {
            player_id: "p3".to_string(),
            player_name: "Thessa".to_string(),
            location: "mouth".to_string(),
        },
        PlayerLocation {
            player_id: "p4".to_string(),
            player_name: "Dace".to_string(),
            location: "antechamber".to_string(),
        },
    ];

    let result = PartyReconciliation::reconcile(&players, false);

    match result {
        ReconciliationResult::Reconciled {
            ref target_location,
            ref players_moved,
            ..
        } => {
            // Deterministic tie-break: alphabetically first location wins
            assert_eq!(
                target_location, "antechamber",
                "Tie-break should pick alphabetically first location"
            );
            assert_eq!(
                players_moved.len(),
                2,
                "Two players should be moved from mouth to antechamber"
            );
        }
        other => panic!("Expected Reconciled for even split, got {:?}", other),
    }
}
