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

    // STRENGTHENED per review finding #8 (2026-04-10). The previous
    // assertion used `contains("positive_suffix") || contains("art_style")`
    // which was satisfied by the identifier `art_style` appearing in the
    // function signature regardless of whether substitution happened. The
    // OR-disjunction made the test pass on mere co-occurrence rather than
    // on the substitution logic itself.
    //
    // The current production idiom at render.rs:140-143 is:
    //   match (vs.lora.as_deref(), vs.lora_trigger.as_deref()) {
    //       (Some(_), Some(trigger)) => trigger.to_string(),
    //       _ => vs.positive_suffix.clone(),
    //   };
    //
    // The test now requires the `trigger.to_string` identifier sequence
    // AND a `match` pattern on `lora_trigger` near it. Both must be
    // present — the substitution cannot pass this test without actually
    // producing a String from the trigger value.
    let has_trigger_binding =
        production_code.contains("Some(trigger)") && production_code.contains("trigger.to_string");
    let has_match_on_trigger = production_code.contains("lora_trigger.as_deref()")
        || production_code.contains("vs.lora_trigger");

    assert!(
        has_trigger_binding,
        "dispatch/render.rs must bind `trigger` from the match arm and \
         produce a String from it (e.g. `Some(trigger) => trigger.to_string()`). \
         This proves the trigger word actually enters the composed style, \
         not just that the identifier appears somewhere in the file. \
         Got has_trigger_binding=false — the substitution pattern is missing \
         or renamed. Per ADR-032, the daemon does NOT auto-prepend trigger \
         words; the substitution must happen in Rust."
    );
    assert!(
        has_match_on_trigger,
        "dispatch/render.rs must match on `lora_trigger` to drive the \
         substitution. Got has_match_on_trigger=false."
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

    // STRENGTHENED per review finding #9 (2026-04-10). Previous version
    // used `contains("oil_painting") || contains("flux-schnell")` — the
    // `flux-schnell` literal was removed in story 35-15's initial impl
    // and the OR disjunction silently masked that removal. Split into
    // two unambiguous assertions without OR.
    assert!(
        production_code.contains("enqueue("),
        "dispatch/render.rs must still call enqueue() — the non-LoRA \
         render path cannot be accidentally deleted"
    );
    assert!(
        production_code.contains("positive_suffix"),
        "dispatch/render.rs must still reference positive_suffix for \
         non-LoRA genres (existing behavior preserved). Without this, \
         the no-LoRA fall-through branch at the `_` match arm cannot \
         produce a base style."
    );
    // The `visual_style: None` fallback at dispatch/render.rs:166-180
    // is a pre-existing silent fallback flagged as a Delivery Finding;
    // it must NOT be fixed in 35-15 but also must NOT be removed.
    // Verify the `oil_painting` literal still exists as the sentinel
    // for that branch. The previous `flux-schnell` literal was
    // intentionally removed (it was never a valid daemon variant —
    // dead string); do NOT re-add it and do NOT use it as an OR
    // disjunction with oil_painting.
    assert!(
        production_code.contains("oil_painting"),
        "dispatch/render.rs must preserve the pre-existing `oil_painting` \
         literal in the None-visual_style fallback. Flagged as a Delivery \
         Finding for a dedicated fix — out of scope for 35-15, but do \
         not remove it either."
    );
    assert!(
        !production_code.contains("flux-schnell"),
        "dispatch/render.rs must NOT contain the `flux-schnell` literal. \
         Story 35-15 removed it (never a valid daemon variant). If this \
         assertion fails, someone re-added the dead string."
    );
}

// ===========================================================================
// REWORK (2026-04-10) — New tests added after Reviewer rejection.
// Covers Reviewer findings #1 (silent no-op warning), #5 (audio.rs mood
// image LoRA inheritance), #6 (path traversal guard). Plus a new test
// added to visual_style_lora_story_35_15_tests.rs for finding #2
// (deny_unknown_fields) and to render_queue_lora_story_35_15_tests.rs
// for finding #10 (strengthen enqueue_signature_accepts_none).
// ===========================================================================

#[test]
fn wiring_dispatch_render_warns_when_lora_has_no_trigger() {
    // Reviewer finding #1 (HIGH): When `vs.lora` is Some but
    // `vs.lora_trigger` is None, the match at dispatch/render.rs:140-143
    // falls through to `positive_suffix.clone()` with no warning. The
    // LoRA loads in the daemon but the trigger word never enters the
    // CLIP prompt — the trained style is loaded but not activated. The
    // RED-phase test file explicitly said "the wiring code should log a
    // warning" and Dev didn't add one. Rule: No Silent Fallbacks.
    //
    // This test enforces that the production code contains a warning
    // emission mechanism in the (Some, None) match branch. Accept either
    // `tracing::warn!` with lora_trigger context OR a WatcherEventBuilder
    // with ValidationWarning severity — both surface to the GM panel.
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // A warning emission must exist AND must be associated with the
    // lora_trigger being missing. The test looks for a tracing::warn!
    // or a ValidationWarning watcher event that mentions lora_trigger.
    let has_tracing_warn =
        production_code.contains("tracing::warn!") && production_code.contains("lora_trigger");
    let has_validation_warning = production_code.contains("ValidationWarning")
        && production_code.contains("lora_trigger");

    assert!(
        has_tracing_warn || has_validation_warning,
        "dispatch/render.rs must emit a warning when `vs.lora` is Some \
         but `vs.lora_trigger` is None — otherwise the LoRA loads but \
         the trained style never activates (silent no-op). Add a \
         dedicated `(Some(lora), None)` match arm with either \
         `tracing::warn!(..., \"lora set without lora_trigger\", ...)` \
         OR `WatcherEventBuilder::new(\"render\", ValidationWarning) \
         .field(\"action\", \"lora_trigger_missing\") \
         .field(\"lora_path\", lora_abs) \
         .send()`. Without this, a YAML typo in `lora_trigger` produces \
         zero visible change and zero diagnostic signal — the exact \
         failure mode `feedback_no_fallbacks` prohibits."
    );
}

#[test]
fn wiring_dispatch_render_validates_lora_path_stays_in_genre_pack_dir() {
    // Reviewer finding #6 (MEDIUM, latent CRITICAL if community genre
    // packs are ever accepted): `dispatch/render.rs:150-157` does
    // `ctx.state.genre_packs_path().join(ctx.genre_slug).join(rel)`
    // with no validation that the resolved path stays inside the genre
    // pack directory. A YAML `lora: ../../../etc/passwd.safetensors`
    // escapes the base dir — PathBuf::join doesn't sanitize.
    //
    // Single-user threat model today (only Keith's machine) but the
    // moment community-submitted genre packs become a thing, this is
    // a path-traversal vulnerability. Epic 35's "Wiring Remediation"
    // charter includes closing security gaps introduced by new wires.
    //
    // Accept either pattern: explicit starts_with() check on the
    // resolved path, OR a RelativePath newtype that rejects `..` at
    // deserialization, OR Path::canonicalize() + prefix check.
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    let has_starts_with_guard =
        production_code.contains(".starts_with(") && production_code.contains("genre_packs_path");
    let has_canonicalize_guard = production_code.contains("canonicalize");
    let has_relative_path_check = production_code.contains("is_absolute")
        || production_code.contains("components().any")
        || production_code.contains("ParentDir");

    assert!(
        has_starts_with_guard || has_canonicalize_guard || has_relative_path_check,
        "dispatch/render.rs must validate that the resolved LoRA path \
         stays within the genre pack directory — otherwise a YAML with \
         `lora: ../../../etc/passwd.safetensors` escapes the base dir \
         silently. Accept any of: \
         (a) `resolved.starts_with(genre_packs_path.join(genre_slug))` \
             after path construction, \
         (b) `resolved.canonicalize()?` plus prefix check, \
         (c) a RelativePath newtype that rejects `..` or `is_absolute()` \
             at deserialization time. \
         Finding #6 flagged this as MEDIUM today (single-user) but \
         CRITICAL if community genre packs are ever accepted."
    );
}

#[test]
fn wiring_dispatch_audio_reads_visual_style_preferred_model() {
    // Reviewer finding #5 (MEDIUM): dispatch/audio.rs:245 hardcodes
    // `"dev"` as the variant for mood-image renders and passes
    // `None, None` for LoRA params — it ignores `vs.preferred_model`,
    // `vs.lora`, and `vs.lora_trigger`. Result: a genre with
    // `preferred_model: schnell` has mood images silently rendered at
    // `dev`; a LoRA-enabled genre has mood images rendered without the
    // LoRA while scene images use it. Visual inconsistency between mood
    // and scene within the same session.
    //
    // The fix is to extract a shared `resolve_render_style` helper (or
    // inline the same logic render.rs uses) so both dispatch paths
    // compose visual style identically. This test enforces that audio.rs
    // reads `preferred_model` from visual_style rather than hardcoding
    // a literal.
    let source = include_str!("../src/dispatch/audio.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    // audio.rs must reference preferred_model from the visual_style
    // context. Accept `vs.preferred_model`, `visual_style.preferred_model`,
    // or a helper function invocation that takes vs and returns the
    // variant.
    let reads_preferred_model = production_code.contains("preferred_model");
    assert!(
        reads_preferred_model,
        "dispatch/audio.rs mood-image enqueue path must read \
         `vs.preferred_model` (not hardcode `\"dev\"`). A genre with \
         `preferred_model: schnell` currently has its mood images \
         silently rendered at dev. Either inline the same read pattern \
         render.rs uses, or extract a shared helper \
         `resolve_render_style(vs, genre_packs_path, genre_slug)` and \
         call it from both dispatch paths."
    );

    // Also: audio.rs should read lora/lora_trigger so mood images on
    // LoRA-enabled genres (caverns_and_claudes, spaghetti_western)
    // actually use the trained style. Otherwise mood images look
    // visually inconsistent with scene images within the same session.
    let reads_lora = production_code.contains("vs.lora") || production_code.contains(".lora.as_");
    assert!(
        reads_lora,
        "dispatch/audio.rs mood-image enqueue path must read `vs.lora` \
         so mood images on LoRA-enabled genres use the same trained \
         style as scene images. Currently mood images silently render \
         without the LoRA — visual inconsistency with scene renders \
         within the same session. Flagged by 4 subagents during review."
    );
}

// ===========================================================================
// 7. Variant wire — companion fix in story 35-15. `visual_style.preferred_model`
//    was previously read in dispatch/render.rs and passed to `enqueue()`
//    where the parameter was prefixed `_image_model` (unused). Now it's
//    renamed to `variant` and flows through to the daemon's RenderParams.
//    This test asserts the production code still references preferred_model
//    from the VisualStyle struct — a regression guard for the companion wire.
// ===========================================================================

#[test]
fn wiring_dispatch_render_passes_preferred_model_as_variant() {
    let source = include_str!("../src/dispatch/render.rs");
    let production_code = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(source);

    assert!(
        production_code.contains("preferred_model"),
        "dispatch/render.rs must reference `vs.preferred_model` to feed \
         the variant wire. Story 35-15 renamed `_image_model` → `variant` \
         and plumbed it through to the daemon; the read site in dispatch \
         must survive."
    );
}

// ===========================================================================
// 7. Architect's gap finding — PromptComposer substitution inline.
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
