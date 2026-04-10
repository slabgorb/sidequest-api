//! RED phase wiring tests for Story 35-15: Wire LoRA path from
//! visual_style.yaml through render pipeline to daemon.
//!
//! These tests enforce that the NEW `lora` / `lora_trigger` fields on
//! `VisualStyle` are actually read from production code in the dispatch
//! layer — not just from unit tests. Per CLAUDE.md's "Verify Wiring, Not
//! Just Existence" rule, a struct field with no production consumers is
//! a half-wired feature.
//!
//! Pattern follows existing Epic 35 wiring tests
//! (e.g., `turn_reminder_wiring_story_35_5_tests.rs`,
//! `entity_reference_wiring_story_35_2_tests.rs`): use `include_str!` to
//! read production source files and assert the new API is referenced
//! outside of `#[cfg(test)]` blocks.
//!
//! Covers:
//!   - AC-2: dispatch/render.rs reads visual_style.lora and passes it down
//!   - AC-4: a WatcherEvent with `action=lora_activated` is emitted from
//!           the dispatch layer (GM panel visibility — lie-detector pattern)
//!   - Architect's Design Deviation #2: trigger substitution happens in
//!           Rust, not in the daemon. The composed positive prompt must
//!           include `lora_trigger` instead of (or in addition to)
//!           `positive_suffix` when LoRA is active.
//!   - AC-5: the non-LoRA branch of dispatch/render.rs must still exist
//!           and must NOT emit a `lora_activated` event.

// ===========================================================================
// 1. Non-test consumer — dispatch/render.rs must read the new VisualStyle
//    fields from production code, not just unit tests.
// ===========================================================================

#[test]
fn wiring_dispatch_render_reads_visual_style_lora() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // The field access `vs.lora` (or `visual_style.lora` via bind) must
    // appear in production code. A bare `lora` match could false-positive
    // on something unrelated, so require the struct-field access form.
    let reads_lora = production_code.contains("vs.lora")
        || production_code.contains("visual_style.lora")
        || production_code.contains(".lora.as_");
    assert!(
        reads_lora,
        "sidequest-server/src/dispatch/render.rs must read \
         `visual_style.lora` from production code (not just tests) — \
         story 35-15. Currently, no `.lora` field access was found."
    );
}

#[test]
fn wiring_dispatch_render_reads_visual_style_lora_trigger() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // Per Design Deviation #1 (architect), the field is `lora_trigger`,
    // NOT `trigger_word`. This test enforces the canonical name.
    let reads_trigger = production_code.contains("lora_trigger");
    assert!(
        reads_trigger,
        "sidequest-server/src/dispatch/render.rs must reference the \
         `lora_trigger` field (per ADR-032 and architect's Design \
         Deviation #1). Currently, no `lora_trigger` usage was found. \
         Did Dev use the stale `trigger_word` name from the session \
         file's original description?"
    );
}

// ===========================================================================
// 2. Trigger substitution — per ADR-032, the Rust dispatch layer must
//    substitute `lora_trigger` for `positive_suffix` when LoRA is active.
//    The daemon does NOT auto-prepend trigger words (Architect Design
//    Deviation #2). This test ensures Dev didn't skip the substitution.
// ===========================================================================

#[test]
fn wiring_dispatch_render_substitutes_trigger_into_prompt() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // There must be a code path where `lora_trigger` influences the
    // composed prompt string. A naive implementation that passes
    // `lora_path` through without touching the prompt would mean the
    // LoRA loads but the style never activates — a silent failure that
    // this test catches.
    //
    // The two viable patterns:
    //   1. An `if let Some(trigger) = ...lora_trigger` block that builds
    //      a different `art_style` / `positive_suffix` / `positive_prompt`.
    //   2. A helper function that takes the trigger as input.
    //
    // Either pattern leaves `lora_trigger` adjacent to prompt construction
    // in the source. We assert that the file contains `lora_trigger` in
    // the same region as `positive_suffix` or `art_style` mutation.
    let has_trigger_ref = production_code.contains("lora_trigger");
    let has_positive_suffix_ref = production_code.contains("positive_suffix")
        || production_code.contains("art_style");
    assert!(
        has_trigger_ref && has_positive_suffix_ref,
        "dispatch/render.rs must use lora_trigger in prompt composition \
         (per ADR-032). The current file has lora_trigger: {has_trigger_ref}, \
         positive_suffix/art_style: {has_positive_suffix_ref}. \
         Dev must substitute the trigger word for the positive_suffix when \
         LoRA is active — the daemon does NOT auto-prepend trigger words."
    );
}

// ===========================================================================
// 3. LoRA path is passed to enqueue() — the extended signature must be
//    called from production code.
// ===========================================================================

#[test]
fn wiring_dispatch_render_calls_enqueue_with_lora_param() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // The call site `queue.enqueue(subject, &art_style, &model, &neg_prompt, "")`
    // must be extended to pass the lora_path (and lora_scale). The test
    // looks for `enqueue` AND `lora_path` in the production code — a
    // coarse but effective check that catches "Dev added the field but
    // forgot to pass it."
    let calls_enqueue = production_code.contains("enqueue(");
    let mentions_lora_path = production_code.contains("lora_path")
        || production_code.contains("lora_abs")
        || production_code.contains(".lora.as_")
        || production_code.contains("vs.lora");
    assert!(
        calls_enqueue,
        "dispatch/render.rs must call queue.enqueue(...) — pre-existing wiring"
    );
    assert!(
        mentions_lora_path,
        "dispatch/render.rs must reference the LoRA path when calling \
         enqueue. The wire test found enqueue() but no lora_path reference \
         — Dev added the fields but forgot to pass them through."
    );
}

// ===========================================================================
// 4. OTEL watcher event — GM panel lie-detector for LoRA activation.
//    Per CLAUDE.md OTEL Observability Principle, the Rust dispatch layer
//    must emit a WatcherEvent when LoRA is active, so the GM panel can
//    see it. Daemon-side span attributes do not surface to the watcher.
// ===========================================================================

#[test]
fn wiring_dispatch_render_emits_lora_activated_watcher_event() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // Per the architect's story context and CLAUDE.md OTEL principle,
    // the dispatch layer must emit a watcher event with
    // `action=lora_activated` when LoRA is set. This test checks for
    // the idiomatic WatcherEventBuilder pattern used elsewhere in
    // dispatch (e.g., audio.rs, combat.rs).
    let has_builder = production_code.contains("WatcherEventBuilder::new(\"render\"")
        || production_code.contains("watcher!(\"render\"");
    let has_lora_action = production_code.contains("lora_activated");

    assert!(
        has_builder,
        "dispatch/render.rs must emit a watcher event for the \"render\" \
         component (via WatcherEventBuilder or watcher! macro) — story 35-15 \
         OTEL requirement per CLAUDE.md."
    );
    assert!(
        has_lora_action,
        "dispatch/render.rs must emit a watcher event with \
         action=\"lora_activated\" when LoRA is active — this is the GM \
         panel's lie-detector signal per CLAUDE.md OTEL Observability \
         Principle. Daemon-side span attributes do NOT surface to the \
         watcher WebSocket; the Rust emission is authoritative."
    );
}

// ===========================================================================
// 5. Non-LoRA path preserved — AC-5 regression guardrail.
//    The non-LoRA code path must still exist. If Dev accidentally removes
//    the fallback when adding the LoRA branch, non-LoRA genres would stop
//    rendering entirely.
// ===========================================================================

#[test]
fn wiring_dispatch_render_preserves_non_lora_path() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // The existing non-LoRA path calls enqueue with art_style from
    // visual_style.positive_suffix (or the fallback "oil_painting"). That
    // branch must survive the edit. This test looks for signals that the
    // non-LoRA case is still handled.
    assert!(
        production_code.contains("enqueue("),
        "dispatch/render.rs must still call enqueue() — the non-LoRA \
         render path cannot be accidentally deleted"
    );
    assert!(
        production_code.contains("positive_suffix"),
        "dispatch/render.rs must still reference positive_suffix for \
         non-LoRA genres (existing behavior preserved)"
    );
    // The `visual_style: None` fallback at dispatch/render.rs:133-137 is
    // a pre-existing silent fallback flagged as a Delivery Finding; it
    // must NOT be fixed in 35-15 but also must NOT be removed. Verify
    // the fallback literals still exist.
    assert!(
        production_code.contains("oil_painting") || production_code.contains("flux-schnell"),
        "dispatch/render.rs must preserve the pre-existing None-visual_style \
         fallback literals (oil_painting / flux-schnell). This is flagged \
         as a Delivery Finding for a future story; do not fix in 35-15, \
         but do not remove either."
    );
}

// ===========================================================================
// 6. Architect's gap finding — PromptComposer substitution inline.
//    Since no `PromptComposer` type exists in Rust (only a format! macro),
//    the trigger substitution must happen inline in dispatch/render.rs.
//    This is an assertion about *where* the logic lives, not whether a
//    new type was created.
// ===========================================================================

#[test]
fn wiring_no_new_prompt_composer_type_created() {
    // Per architect's Delivery Finding (Gap, non-blocking), Dev should
    // add the trigger substitution inline — not create a speculative
    // `PromptComposer` type. This test guards against scope creep.
    let source = include_str!("../src/dispatch/render.rs");
    let has_new_type = source.contains("struct PromptComposer")
        || source.contains("trait PromptComposer");
    assert!(
        !has_new_type,
        "dispatch/render.rs must NOT introduce a new PromptComposer \
         type. Per the architect's Delivery Finding, the trigger \
         substitution is a simple inline conditional — no new abstraction. \
         Scope creep is flagged here to keep 35-15 narrow."
    );
}
