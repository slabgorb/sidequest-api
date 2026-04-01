//! Story 15-7: RAG pipeline end-to-end wiring tests.
//!
//! Tests that:
//! 1. `select_lore_for_prompt()` prefers semantic search when embeddings are available
//! 2. `accumulate_lore()` wiring prerequisites are correct
//! 3. OTEL-relevant metadata is present on fragments created by `accumulate_lore()`

use std::collections::HashMap;
use sidequest_game::lore::{
    accumulate_lore, cosine_similarity, select_lore_for_prompt, LoreCategory, LoreFragment,
    LoreSource, LoreStore,
};

// ============================================================
// Helpers
// ============================================================

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

fn make_embedding(seed: f32, dim: usize) -> Vec<f32> {
    // Create a simple normalized embedding vector
    let raw: Vec<f32> = (0..dim).map(|i| (seed + i as f32).sin()).collect();
    let mag: f32 = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag == 0.0 {
        raw
    } else {
        raw.iter().map(|x| x / mag).collect()
    }
}

// ============================================================
// AC-4: select_lore_for_prompt prefers query_by_similarity when embeddings available
// ============================================================

#[test]
fn select_lore_prefers_semantic_when_embeddings_available() {
    // Build a store where keyword matching and semantic matching disagree.
    // Keyword hint "mountain" matches geo-keyword but NOT geo-semantic.
    // Semantic similarity to query should prefer geo-semantic.
    let mut store = LoreStore::new();

    // Fragment that matches keyword "mountain" but is semantically distant
    let keyword_match = make_fragment(
        "geo-keyword",
        LoreCategory::Geography,
        "The mountain pass is treacherous and cold, with winds that cut like knives.",
    );
    store.add(keyword_match).unwrap();

    // Fragment that does NOT match keyword "mountain" but is semantically close
    // (about alpine terrain, peaks, elevation — same semantic domain)
    let semantic_match = make_fragment(
        "geo-semantic",
        LoreCategory::Geography,
        "The high alpine peaks rise above the clouds, their glacial ridges gleaming in sunlight.",
    )
    .with_embedding(make_embedding(1.0, 384));
    store.add(semantic_match).unwrap();

    // Another fragment with an embedding, semantically distant
    let distant = make_fragment(
        "fac-001",
        LoreCategory::Faction,
        "The Iron Collective controls the water supply through rationing and fear.",
    )
    .with_embedding(make_embedding(99.0, 384));
    store.add(distant).unwrap();

    // Query embedding is close to geo-semantic's embedding
    let query_embedding = make_embedding(1.05, 384);

    // When embeddings are available, select_lore_for_prompt should use semantic search
    // and prefer the semantically-close fragment over the keyword match.
    let selected = select_lore_for_prompt(&store, 500, None, Some(&query_embedding));

    // The semantically-close fragment should appear before the keyword-only match
    assert!(
        !selected.is_empty(),
        "Should select at least one fragment"
    );

    // Find positions of both fragments
    let semantic_pos = selected.iter().position(|f| f.id() == "geo-semantic");
    let keyword_pos = selected.iter().position(|f| f.id() == "geo-keyword");

    assert!(
        semantic_pos.is_some(),
        "Semantic match should be selected"
    );
    // When embeddings are available, semantic match should be ranked first
    if let (Some(sem), Some(kw)) = (semantic_pos, keyword_pos) {
        assert!(
            sem < kw,
            "Semantic match (pos {sem}) should rank before keyword match (pos {kw})"
        );
    }
}

#[test]
fn select_lore_falls_back_to_category_ranking_when_no_embeddings() {
    // When NO fragments have embeddings, category-based ranking should work
    let mut store = LoreStore::new();

    store
        .add(make_fragment(
            "geo-001",
            LoreCategory::Geography,
            "The mountain pass connects the northern and southern kingdoms.",
        ))
        .unwrap();
    store
        .add(make_fragment(
            "hist-001",
            LoreCategory::History,
            "The ancient empire fell a thousand years ago in a great cataclysm.",
        ))
        .unwrap();

    let selected = select_lore_for_prompt(&store, 500, None, None);

    assert!(!selected.is_empty());
    // Geography gets priority 1, History gets priority 2 — Geography should be first
    assert_eq!(selected[0].id(), "geo-001");
}

#[test]
fn select_lore_accepts_query_embedding_parameter() {
    // Verify the function signature accepts an optional query embedding.
    // This tests the API contract — the function must accept the new parameter.
    let store = LoreStore::new();
    let query = make_embedding(1.0, 384);

    // Should compile and not panic with empty store
    let result = select_lore_for_prompt(&store, 100, None, Some(&query));
    assert!(result.is_empty());

    // Also works with None embedding (backward compat)
    let result = select_lore_for_prompt(&store, 100, None, None);
    assert!(result.is_empty());
}

// ============================================================
// AC-1: accumulate_lore produces fragments with correct metadata for OTEL
// ============================================================

#[test]
fn accumulate_lore_sets_turn_and_category_for_otel() {
    let mut store = LoreStore::new();
    let meta = HashMap::new();

    let _id = accumulate_lore(
        &mut store,
        "The ancient temple collapsed during the earthquake.",
        LoreCategory::Event,
        42,
        meta,
    )
    .expect("accumulate_lore should succeed");

    // Verify the fragment was stored with correct metadata for OTEL
    let events = store.query_by_category(&LoreCategory::Event);
    assert_eq!(events.len(), 1, "Should have exactly one event fragment");
    let fragment = events[0];

    // OTEL event lore.fragment_accumulated needs: category, turn, token_estimate
    assert_eq!(fragment.category(), &LoreCategory::Event);
    assert_eq!(fragment.turn_created(), Some(42));
    assert!(
        fragment.token_estimate() > 0,
        "Token estimate should be positive for OTEL reporting"
    );
}

#[test]
fn accumulate_lore_rejects_empty_description() {
    let mut store = LoreStore::new();
    let result = accumulate_lore(&mut store, "", LoreCategory::Event, 1, HashMap::new());
    assert!(result.is_err(), "Empty description should be rejected");
}

// ============================================================
// Semantic search infrastructure works correctly
// ============================================================

#[test]
fn query_by_similarity_returns_ranked_results() {
    let mut store = LoreStore::new();

    // Add fragments with embeddings at known similarity distances
    let close = make_fragment("close", LoreCategory::History, "Nearby lore")
        .with_embedding(make_embedding(1.0, 384));
    let far = make_fragment("far", LoreCategory::History, "Distant lore")
        .with_embedding(make_embedding(50.0, 384));
    let no_emb = make_fragment("none", LoreCategory::History, "No embedding lore");

    store.add(close).unwrap();
    store.add(far).unwrap();
    store.add(no_emb).unwrap();

    let query = make_embedding(1.01, 384);
    let results = store.query_by_similarity(&query, 10);

    // Should skip fragment without embedding
    assert_eq!(results.len(), 2, "Should return only fragments with embeddings");

    // "close" should rank higher than "far"
    assert_eq!(results[0].0.id(), "close");
    assert_eq!(results[1].0.id(), "far");
    assert!(
        results[0].1 > results[1].1,
        "Close fragment should have higher similarity score"
    );
}

#[test]
fn cosine_similarity_identical_vectors_returns_one() {
    let v = make_embedding(1.0, 384);
    let sim = cosine_similarity(&v, &v);
    assert!(
        (sim - 1.0).abs() < 1e-5,
        "Identical vectors should have similarity ~1.0, got {sim}"
    );
}

#[test]
fn cosine_similarity_orthogonal_returns_near_zero() {
    // Construct actually orthogonal vectors: one on even indices, one on odd
    let mut a = vec![0.0f32; 384];
    let mut b = vec![0.0f32; 384];
    for i in (0..384).step_by(2) {
        a[i] = 1.0;
    }
    for i in (1..384).step_by(2) {
        b[i] = 1.0;
    }
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim.abs() < 1e-5,
        "Orthogonal vectors should have similarity ~0.0, got {sim}"
    );
}
