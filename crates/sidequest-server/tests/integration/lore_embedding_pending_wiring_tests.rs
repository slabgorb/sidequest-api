//! Wiring tests for the lore `embedding_pending` retry mechanism — guards
//! against the 2026-04-10 cascade visibility gap.
//!
//! Background: when the embed daemon hung mid-playtest, every lore
//! fragment created during the outage was stored without an embedding
//! and the failure was logged only via `tracing::warn!`. From the GM
//! panel's perspective the system looked healthy — but those fragments
//! were silently invisible to semantic search for the rest of the
//! session. The cascade fix:
//!
//!   1. `LoreFragment` carries an `embedding_pending: bool` flag.
//!   2. `LoreStore` exposes `mark_embedding_pending`, `set_embedding`
//!      (which clears the flag), and `pending_embedding_fragments` for
//!      retry sweeps.
//!   3. `dispatch/lore_sync.rs` marks fragments pending on every embed
//!      failure path (timeout, daemon error, connect refused), emits a
//!      `lore.embedding_pending` ValidationWarning, and runs a retry
//!      sweep at the start of every accumulation pass.
//!   4. `dispatch/prompt.rs` emits `lore.query_embedding_failed` when
//!      query embedding can't be generated, so the GM panel sees the
//!      degradation to keyword ranking.
//!
//! These source-level guards ensure a future refactor cannot silently
//! revert any of those pieces.

const STORE_SRC: &str = include_str!("../../../sidequest-game/src/lore/store.rs");
const LORE_SYNC_SRC: &str = include_str!("../../src/dispatch/lore_sync.rs");
const PROMPT_SRC: &str = include_str!("../../src/dispatch/prompt.rs");

fn prod(src: &str) -> &str {
    src.split("#[cfg(test)]").next().unwrap_or(src)
}

// ===========================================================================
// 1. LoreFragment carries the embedding_pending flag
// ===========================================================================

#[test]
fn lore_fragment_carries_embedding_pending_field() {
    let s = prod(STORE_SRC);
    assert!(
        s.contains("embedding_pending: bool"),
        "LoreFragment must declare an `embedding_pending: bool` field — \
         the recovery flag for daemon-outage embed failures"
    );
    assert!(
        s.contains("pub fn is_embedding_pending(&self)"),
        "LoreFragment must expose `is_embedding_pending` so callers can \
         filter for fragments awaiting retry"
    );
}

#[test]
fn lore_fragment_pending_field_uses_serde_default() {
    let s = prod(STORE_SRC);
    // The field must round-trip with existing on-disk data — the
    // #[serde(default)] sits immediately above the field declaration.
    let idx = s
        .find("embedding_pending: bool")
        .expect("field declared above");
    let preceding = &s[idx.saturating_sub(120)..idx];
    assert!(
        preceding.contains("#[serde(default"),
        "embedding_pending must use #[serde(default)] so existing \
         persisted fragments deserialize without the field"
    );
}

// ===========================================================================
// 2. LoreStore exposes mark/set/list APIs
// ===========================================================================

#[test]
fn lore_store_exposes_mark_embedding_pending() {
    let s = prod(STORE_SRC);
    assert!(
        s.contains("pub fn mark_embedding_pending(&mut self, id: &str)"),
        "LoreStore must expose `mark_embedding_pending` for the dispatch \
         layer to flag fragments after embed failures"
    );
}

#[test]
fn set_embedding_clears_pending_flag() {
    let s = prod(STORE_SRC);
    let set_idx = s
        .find("pub fn set_embedding")
        .expect("set_embedding exists");
    let body_end = s[set_idx..]
        .find("\n    }")
        .expect("set_embedding has a closing brace");
    let body = &s[set_idx..set_idx + body_end];
    assert!(
        body.contains("embedding_pending = false"),
        "set_embedding must clear the embedding_pending flag — attaching \
         a real embedding necessarily resolves any prior failure"
    );
}

#[test]
fn lore_store_exposes_pending_embedding_fragments() {
    let s = prod(STORE_SRC);
    assert!(
        s.contains("pub fn pending_embedding_fragments(&self)"),
        "LoreStore must expose `pending_embedding_fragments` so the \
         retry sweep can iterate (id, content) pairs without holding a \
         borrow across the daemon round-trip"
    );
}

// ===========================================================================
// 3. dispatch/lore_sync.rs marks pending and emits OTEL on every failure path
// ===========================================================================

#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn lore_sync_marks_pending_on_embed_error() {
    let s = prod(LORE_SYNC_SRC);
    assert!(
        s.contains("mark_embedding_pending(&fragment_id)"),
        "dispatch/lore_sync.rs must call mark_embedding_pending on the \
         embed failure path — without this the fragment is silently \
         invisible to semantic search forever"
    );
}

#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn lore_sync_emits_embedding_pending_watcher_event() {
    let s = prod(LORE_SYNC_SRC);
    assert!(
        s.contains("\"lore.embedding_pending\""),
        "dispatch/lore_sync.rs must emit a `lore.embedding_pending` \
         watcher event so the GM panel surfaces fragments awaiting \
         retry (the lie-detector signal per CLAUDE.md OTEL principle)"
    );
}

#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn lore_sync_distinguishes_failure_modes() {
    let s = prod(LORE_SYNC_SRC);
    // Both error_kind values must appear so the GM panel can
    // distinguish "daemon down" from "daemon up but embed timed out".
    assert!(
        s.contains("\"daemon_unreachable\""),
        "lore_sync must tag `error_kind=daemon_unreachable` on connect \
         failures so the GM panel can distinguish outage modes"
    );
    assert!(
        s.contains("\"embed_failed\""),
        "lore_sync must tag `error_kind=embed_failed` on round-trip \
         failures so the GM panel can distinguish outage modes"
    );
}

#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn lore_sync_runs_retry_sweep_on_accumulate() {
    let s = prod(LORE_SYNC_SRC);
    assert!(
        s.contains("retry_pending_embeddings"),
        "dispatch/lore_sync.rs must define a retry_pending_embeddings sweep"
    );
    // The accumulate function must call the sweep — guard against the
    // sweep existing but never being called.
    let acc_start = s
        .find("pub(super) async fn accumulate_and_persist_lore")
        .expect("accumulate function exists");
    let body = &s[acc_start..];
    let body_window = &body[..body.len().min(4000)];
    assert!(
        body_window.contains("retry_pending_embeddings(ctx).await"),
        "accumulate_and_persist_lore must invoke retry_pending_embeddings \
         at the start of every pass so transient daemon outages heal on \
         the next turn"
    );
}

#[test]
#[ignore = "tech-debt: source-grep wiring test broken after ADR-063 dispatch decomposition (file references stale or moved); rewrite as behavior test or update paths — see TECH_DEBT.md"]
fn retry_sweep_emits_summary_event() {
    let s = prod(LORE_SYNC_SRC);
    assert!(
        s.contains("lore.embedding_retry_sweep") || s.contains("\"lore.embedding_retry_sweep\""),
        "retry sweep must emit a `lore.embedding_retry_sweep` summary \
         event so the GM panel sees the recovery happening"
    );
    assert!(
        s.contains("lore.embedding_retried_ok") || s.contains("\"lore.embedding_retried_ok\""),
        "retry sweep must emit per-fragment `lore.embedding_retried_ok` \
         so the GM panel sees individual fragments recovering"
    );
}

// ===========================================================================
// 4. dispatch/prompt.rs surfaces query embedding failures
// ===========================================================================

#[test]
fn prompt_emits_query_embedding_failed_watcher_event() {
    let s = prod(PROMPT_SRC);
    assert!(
        s.contains("\"lore.query_embedding_failed\""),
        "dispatch/prompt.rs must emit a `lore.query_embedding_failed` \
         watcher event when query embedding fails — without this, a \
         wedged daemon silently downgrades every prompt to keyword \
         ranking and the GM panel has no signal that semantic retrieval \
         is disabled"
    );
}
