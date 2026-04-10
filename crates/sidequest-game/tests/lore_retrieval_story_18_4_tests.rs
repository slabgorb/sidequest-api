//! Story 18-4: LoreRetrievalSummary struct and telemetry data tests.
//!
//! Tests that:
//! 1. `LoreRetrievalSummary` struct exists with selected/rejected fragments
//! 2. `summarize_lore_retrieval()` produces correct telemetry from select results
//! 3. Summary serializes to JSON for WatcherEvent fields

use sidequest_game::lore::{
    LoreCategory, LoreFragment, LoreRetrievalSummary, LoreSource, LoreStore,
};
use std::collections::HashMap;

fn make_fragment(id: &str, category: LoreCategory, content: &str) -> LoreFragment {
    LoreFragment::new(
        id.to_string(),
        category,
        content.to_string(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    )
}

fn build_test_store() -> LoreStore {
    let mut store = LoreStore::new();
    // ~25 tokens each (100 chars / 4)
    store
        .add(make_fragment(
            "hist-001",
            LoreCategory::History,
            "The great war of the ancients lasted a thousand years and reshaped the continent forever more.",
        ))
        .unwrap();
    store
        .add(make_fragment(
            "geo-001",
            LoreCategory::Geography,
            "The Flickering Reach spans the northern wastes, a desolate expanse of irradiated terrain.",
        ))
        .unwrap();
    store
        .add(make_fragment(
            "fac-001",
            LoreCategory::Faction,
            "The Iron Collective controls the water supply and rules through rationing and fear always.",
        ))
        .unwrap();
    store
        .add(make_fragment(
            "char-001",
            LoreCategory::Character,
            "Old Mara remembers the world before the bombs and tells stories to the wasteland children.",
        ))
        .unwrap();
    store
        .add(make_fragment(
            "item-001",
            LoreCategory::Item,
            "The Geiger Lantern glows brighter as radiation increases, a vital survival tool for scouts.",
        ))
        .unwrap();
    store
}

// ============================================================
// AC-1: LoreRetrievalSummary struct exists
// ============================================================

#[test]
fn lore_retrieval_summary_struct_exists() {
    let summary = LoreRetrievalSummary {
        budget: 500,
        tokens_used: 0,
        selected: vec![],
        rejected: vec![],
        context_hint: None,
        total_fragments: 0,
    };
    assert_eq!(summary.budget, 500);
    assert_eq!(summary.tokens_used, 0);
    assert!(summary.selected.is_empty());
    assert!(summary.rejected.is_empty());
}

// ============================================================
// AC-2: summarize_lore_retrieval produces correct data
// ============================================================

#[test]
fn summarize_captures_selected_and_rejected() {
    let store = build_test_store();
    // Budget of 60 tokens — should select ~2 fragments (each ~25 tokens)
    let selected = sidequest_game::select_lore_for_prompt(&store, 60, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 60, None);

    assert_eq!(summary.budget, 60);
    assert_eq!(summary.total_fragments, 5);
    assert!(
        !summary.selected.is_empty(),
        "should have selected fragments"
    );
    assert!(
        !summary.rejected.is_empty(),
        "should have rejected fragments"
    );
    assert_eq!(
        summary.selected.len() + summary.rejected.len(),
        5,
        "selected + rejected should equal total"
    );
}

#[test]
fn summarize_tokens_used_matches_selected() {
    let store = build_test_store();
    let selected = sidequest_game::select_lore_for_prompt(&store, 60, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 60, None);

    let expected_tokens: usize = summary.selected.iter().map(|f| f.tokens).sum();
    assert_eq!(
        summary.tokens_used, expected_tokens,
        "tokens_used should equal sum of selected fragment tokens"
    );
    assert!(
        summary.tokens_used <= summary.budget,
        "tokens_used should not exceed budget"
    );
}

#[test]
fn summarize_with_priority_categories() {
    let store = build_test_store();
    let cats = [LoreCategory::Geography];
    let selected = sidequest_game::select_lore_for_prompt(&store, 60, Some(&cats), None);
    let summary =
        sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 60, Some(&cats));

    assert!(
        summary.context_hint.is_some(),
        "should have priority category hint"
    );
    // Geography fragment should be selected first
    assert!(
        summary.selected.iter().any(|f| f.category == "geography"),
        "Geography should be prioritized"
    );
}

#[test]
fn summarize_large_budget_selects_all() {
    let store = build_test_store();
    let selected = sidequest_game::select_lore_for_prompt(&store, 10000, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 10000, None);

    assert_eq!(summary.selected.len(), 5, "should select all fragments");
    assert!(
        summary.rejected.is_empty(),
        "should reject none with large budget"
    );
    assert_eq!(summary.tokens_used, store.total_tokens());
}

#[test]
fn summarize_zero_budget_rejects_all() {
    let store = build_test_store();
    let selected = sidequest_game::select_lore_for_prompt(&store, 0, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 0, None);

    assert!(summary.selected.is_empty(), "should select none");
    assert_eq!(
        summary.rejected.len(),
        5,
        "should reject all with zero budget"
    );
    assert_eq!(summary.tokens_used, 0);
}

// ============================================================
// AC-3: Fragment summaries contain id, category, tokens
// ============================================================

#[test]
fn fragment_summary_has_required_fields() {
    let store = build_test_store();
    let selected = sidequest_game::select_lore_for_prompt(&store, 10000, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 10000, None);

    for frag in &summary.selected {
        assert!(!frag.id.is_empty(), "fragment id should not be empty");
        assert!(frag.tokens > 0, "fragment tokens should be positive");
        // category is a string representation
        assert!(
            !frag.category.is_empty(),
            "fragment category should not be empty"
        );
    }
}

// ============================================================
// AC-4: LoreRetrievalSummary serializes to JSON
// ============================================================

#[test]
fn summary_serializes_to_json() {
    let store = build_test_store();
    let selected = sidequest_game::select_lore_for_prompt(&store, 60, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 60, None);

    let json = serde_json::to_value(&summary).unwrap();
    assert!(json["budget"].is_number());
    assert!(json["tokens_used"].is_number());
    assert!(json["selected"].is_array());
    assert!(json["rejected"].is_array());
    assert!(json["total_fragments"].is_number());

    let selected_arr = json["selected"].as_array().unwrap();
    if !selected_arr.is_empty() {
        assert!(selected_arr[0]["id"].is_string());
        assert!(selected_arr[0]["category"].is_string());
        assert!(selected_arr[0]["tokens"].is_number());
    }
}

// ============================================================
// AC-5: Empty store produces valid summary
// ============================================================

#[test]
fn empty_store_produces_valid_summary() {
    let store = LoreStore::new();
    let selected = sidequest_game::select_lore_for_prompt(&store, 500, None, None);
    let summary = sidequest_game::lore::summarize_lore_retrieval(&store, &selected, 500, None);

    assert_eq!(summary.budget, 500);
    assert_eq!(summary.tokens_used, 0);
    assert!(summary.selected.is_empty());
    assert!(summary.rejected.is_empty());
    assert_eq!(summary.total_fragments, 0);
}
