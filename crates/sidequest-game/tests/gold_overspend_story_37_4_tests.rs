//! Story 37-4: Gold overspend — spend_gold must reject transactions that exceed balance.
//!
//! The current `spend_gold` silently clamps to available balance instead of rejecting.
//! These tests assert the correct behavior: overspend attempts return Err and leave
//! balance unchanged.

use sidequest_game::Inventory;

// ── Core contract: spend_gold returns Result ─────────────────────────

#[test]
fn spend_gold_exact_amount_returns_ok() {
    let mut inv = Inventory {
        items: vec![],
        gold: 50,
    };
    let result = inv.spend_gold(13);
    assert!(result.is_ok(), "exact spend within balance should succeed");
    assert_eq!(result.unwrap(), 13, "should report amount spent");
    assert_eq!(inv.gold, 37, "balance should be reduced by spent amount");
}

#[test]
fn spend_gold_exact_balance_returns_ok() {
    let mut inv = Inventory {
        items: vec![],
        gold: 10,
    };
    let result = inv.spend_gold(10);
    assert!(result.is_ok(), "spending exact balance should succeed");
    assert_eq!(result.unwrap(), 10);
    assert_eq!(inv.gold, 0);
}

#[test]
fn spend_gold_overspend_returns_err() {
    let mut inv = Inventory {
        items: vec![],
        gold: 10,
    };
    let result = inv.spend_gold(13);
    assert!(
        result.is_err(),
        "spending more than available gold must return Err, not silently clamp"
    );
}

#[test]
fn spend_gold_overspend_leaves_balance_unchanged() {
    let mut inv = Inventory {
        items: vec![],
        gold: 10,
    };
    let _ = inv.spend_gold(13);
    assert_eq!(
        inv.gold, 10,
        "failed transaction must not modify gold balance"
    );
}

#[test]
fn spend_gold_from_zero_returns_err() {
    let mut inv = Inventory {
        items: vec![],
        gold: 0,
    };
    let result = inv.spend_gold(5);
    assert!(
        result.is_err(),
        "spending from zero balance must return Err"
    );
}

#[test]
fn spend_gold_from_zero_leaves_balance_at_zero() {
    let mut inv = Inventory {
        items: vec![],
        gold: 0,
    };
    let _ = inv.spend_gold(5);
    assert_eq!(
        inv.gold, 0,
        "zero balance must remain zero after failed spend"
    );
}

// ── Negative amount guard ────────────────────────────────────────────

#[test]
fn spend_gold_negative_amount_returns_err() {
    let mut inv = Inventory {
        items: vec![],
        gold: 50,
    };
    let result = inv.spend_gold(-10);
    assert!(
        result.is_err(),
        "negative spend amount is nonsensical and must be rejected"
    );
}

// ── Zero spend edge case ─────────────────────────────────────────────

#[test]
fn spend_gold_zero_amount_returns_ok() {
    let mut inv = Inventory {
        items: vec![],
        gold: 50,
    };
    let result = inv.spend_gold(0);
    assert!(result.is_ok(), "spending zero gold should succeed");
    assert_eq!(result.unwrap(), 0);
    assert_eq!(inv.gold, 50, "balance unchanged on zero spend");
}
