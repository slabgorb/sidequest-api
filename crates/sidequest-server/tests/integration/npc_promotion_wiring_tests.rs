//! Wiring test — verifies NPC resolution-tier promotion helpers are actually
//! called from `update_npc_registry` in production code.
//!
//! `evaluate_promotion` and `log_promotion_event` previously existed as
//! `pub fn`s with `#[allow(dead_code)]` FIXME comments marking them as
//! unwired sq-wire-it debt. The Phase F lint sweep pulled them into the
//! real registry-update path; this test guards against regressions that
//! would silently disconnect the promotion flow again.

const NPC_REGISTRY_SRC: &str = include_str!("../../src/dispatch/npc_registry.rs");

/// Return the body of `fn update_npc_registry` as a &str slice, stripping
/// `#[cfg(test)]` content so test-only calls don't create false positives.
fn update_npc_registry_body() -> &'static str {
    let production = NPC_REGISTRY_SRC
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(NPC_REGISTRY_SRC);
    let fn_start = production
        .find("fn update_npc_registry(")
        .expect("fn update_npc_registry must exist in dispatch/npc_registry.rs");
    let rest = &production[fn_start..];
    // Body extends until the next top-level `fn ` at column 0 (the helper
    // functions below it). Good enough for a grep test.
    let body_end = rest[1..]
        .find("\nfn ")
        .or_else(|| rest[1..].find("\npub fn "))
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    &rest[..body_end]
}

#[test]
fn update_npc_registry_increments_non_transactional_interactions() {
    let body = update_npc_registry_body();
    assert!(
        body.contains("non_transactional_interactions"),
        "update_npc_registry() must reference non_transactional_interactions \
         so every narrator NPC mention counts toward tier promotion — \
         Phase F layered content wiring"
    );
    assert!(
        body.contains("saturating_add"),
        "update_npc_registry() must use saturating_add when incrementing \
         non_transactional_interactions to avoid u32 wraparound on \
         pathologically long sessions — Phase F layered content wiring"
    );
}

#[test]
fn update_npc_registry_calls_evaluate_promotion() {
    let body = update_npc_registry_body();
    assert!(
        body.contains("evaluate_promotion("),
        "update_npc_registry() must call evaluate_promotion() per-NPC so \
         Spawn → Engage → Promote transitions actually fire. A pub fn \
         without a production caller is a half-wired feature (per \
         CLAUDE.md no-half-wired rule)."
    );
}

#[test]
fn update_npc_registry_calls_log_promotion_event() {
    let body = update_npc_registry_body();
    assert!(
        body.contains("log_promotion_event("),
        "update_npc_registry() must call log_promotion_event() when \
         evaluate_promotion returns a new tier, so the GM panel sees \
         the transition in its telemetry stream."
    );
}

#[test]
fn promotion_helpers_no_longer_allow_dead_code() {
    // Defensive: the prior session had `#[allow(dead_code)]` + FIXME
    // comments masking the unwired state. If those ever come back, this
    // test fires — they mean the real call-site regressed.
    assert!(
        !NPC_REGISTRY_SRC.contains("#[allow(dead_code)]"),
        "npc_registry.rs must not contain #[allow(dead_code)] — if a \
         function looks dead, wire it or delete it (no half-wired \
         features, per CLAUDE.md)."
    );
    assert!(
        !NPC_REGISTRY_SRC.contains("FIXME:"),
        "npc_registry.rs must not carry FIXME markers — FIXMEs on \
         committed code are sq-wire-it debt. Resolve before merging."
    );
}
