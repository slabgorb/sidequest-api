//! RED phase tests for Story 35-15: Wire LoRA path from visual_style.yaml
//! through render pipeline to daemon.
//!
//! Covers the `sidequest-game::render_queue` layer of the wire:
//! `RenderQueue::enqueue()` must accept `lora_path` and `lora_scale`
//! parameters, and the background worker closure must receive them.
//!
//! The existing `RenderQueue::spawn` closure signature at
//! `render_queue.rs:305-307` takes 7 positional args:
//! `Fn(String, String, String, String, String, u32, u32) -> Fut`.
//!
//! Per the architect's Delivery Finding (Improvement, non-blocking),
//! this closure should eventually be refactored to take a `RenderParams`
//! struct instead of 9 positional args, but for 35-15 we extend the
//! positional signature — story scope is "wire it, don't refactor it."
//!
//! These tests capture the extended closure arguments via
//! `Arc<Mutex<Vec<...>>>` so we can observe what reached the worker.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sidequest_game::render_queue::{
    EnqueueResult, RenderQueue, RenderQueueConfig,
};
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};

// ─────────────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────────────

fn make_subject(tag: &str) -> RenderSubject {
    RenderSubject::new(
        vec![tag.to_string()],
        SceneType::Exploration,
        SubjectTier::Scene,
        format!("{tag} in a dramatic scene"),
        0.8,
    )
    .expect("Test fixture: valid RenderSubject")
}

fn test_config() -> RenderQueueConfig {
    RenderQueueConfig::new(8, 4, Duration::from_secs(60))
        .expect("valid config")
}

/// Spawns a `RenderQueue` whose worker captures every call into the
/// supplied `captures` vec, returning the queue. Eliminates ~25 lines
/// of closure boilerplate that would otherwise repeat across every test
/// needing to inspect what reached the worker. The mock render result
/// is `(format!("/tmp/mock/{prompt}.png"), 42u64)` — tests that need
/// alternative return semantics build their own closure inline.
///
/// Extracted in the verify phase per the simplify-reuse high-confidence
/// finding for story 35-15.
fn make_capturing_queue(captures: Arc<Mutex<Vec<CapturedCall>>>) -> RenderQueue {
    RenderQueue::spawn(
        test_config(),
        move |prompt: String,
              _art_style: String,
              _tier: String,
              _neg: String,
              _narration: String,
              _w: u32,
              _h: u32,
              variant: String,
              lora_path: Option<String>,
              lora_scale: Option<f32>| {
            let captures = Arc::clone(&captures);
            async move {
                captures.lock().unwrap().push(CapturedCall {
                    prompt: prompt.clone(),
                    variant,
                    lora_path: lora_path.clone(),
                    lora_scale,
                });
                Ok((format!("/tmp/mock/{prompt}.png"), 42u64))
            }
        },
    )
}

/// Captured call arguments from the worker closure.
///
/// Dev extended the `render_fn` signature to take 10 positional args:
/// `|prompt, style, tier, neg, narration, w, h, variant, lora_path, lora_scale|`.
/// This fixture records what each render received. `variant` is the
/// Flux model override ("dev" / "schnell" / ""), previously dropped at
/// the dead `_image_model` parameter. The trailing lora fields use
/// `Option` to distinguish "not sent" from "sent with a value."
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CapturedCall {
    prompt: String,
    variant: String,
    lora_path: Option<String>,
    lora_scale: Option<f32>,
}

// ─────────────────────────────────────────────────────────────────────────
// AC-5 regression guardrail — enqueue without LoRA leaves fields None
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn enqueue_without_lora_passes_none_to_worker() {
    // REGRESSION TEST — written FIRST per SM assessment. A genre without
    // a trained LoRA must reach the worker with `lora_path: None`, not
    // `Some("")` or `Some("base")` or any other silent fallback.
    //
    // This test uses the EXTENDED closure signature. If Dev doesn't add
    // the two extra positional args, this test fails to compile — that's
    // a legitimate RED state.
    let captures: Arc<Mutex<Vec<CapturedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let queue = make_capturing_queue(Arc::clone(&captures));

    let subject = make_subject("non_lora_genre");
    // Empty variant → daemon falls back to tier default. This is the
    // correct "absence of override" contract per story 35-15's wiring fix.
    let result = queue
        .enqueue(subject, "oil_painting", "", "", "", None, None)
        .await
        .expect("enqueue must succeed");

    assert!(matches!(result, EnqueueResult::Queued { .. }));

    // Wait briefly for the background worker to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    let calls = captures.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "worker must have received exactly one call — got {}",
        calls.len()
    );
    assert!(
        calls[0].lora_path.is_none(),
        "worker must receive lora_path: None for non-LoRA enqueue, got {:?}",
        calls[0].lora_path
    );
    assert!(
        calls[0].lora_scale.is_none(),
        "worker must receive lora_scale: None for non-LoRA enqueue, got {:?}",
        calls[0].lora_scale
    );
    // Variant regression — an empty variant string must survive the wire
    // verbatim as "" (not silently defaulted to "dev" or "schnell").
    // The daemon is the single source of truth for the tier fallback.
    assert_eq!(
        calls[0].variant, "",
        "worker must receive variant: \"\" for no-override enqueue — \
         Rust must not silently default. Got {:?}",
        calls[0].variant
    );
}

// ─────────────────────────────────────────────────────────────────────────
// AC-2 positive path — enqueue forwards LoRA params to worker
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn enqueue_with_lora_path_forwards_to_worker() {
    let captures: Arc<Mutex<Vec<CapturedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let queue = make_capturing_queue(Arc::clone(&captures));

    let subject = make_subject("spaghetti_western_entity");
    let lora_abs = "/abs/genre_packs/spaghetti_western/lora/sw_style.safetensors";
    let result = queue
        .enqueue(
            subject,
            "sw_style",
            "dev", // variant — story 35-15 closed the dead wire; "dev" is a
                   // canonical Flux variant, matching the daemon's TIER_CONFIGS
                   // vocabulary. Previously passed as "flux-dev" (dead string).
            "",
            "",
            Some(lora_abs),
            Some(0.85),
        )
        .await
        .expect("enqueue with lora must succeed");

    assert!(matches!(result, EnqueueResult::Queued { .. }));

    tokio::time::sleep(Duration::from_millis(50)).await;

    let calls = captures.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);

    assert_eq!(
        calls[0].lora_path.as_deref(),
        Some(lora_abs),
        "worker must receive the lora_path verbatim from enqueue"
    );
    assert_eq!(
        calls[0].lora_scale,
        Some(0.85),
        "worker must receive the lora_scale verbatim from enqueue"
    );
    // Variant must also survive the wire. Pre-story-35-15 this was
    // silently dropped at `_image_model`; now the value reaches the
    // worker closure (and onward to RenderParams.variant for the daemon).
    assert_eq!(
        calls[0].variant, "dev",
        "worker must receive variant: \"dev\" verbatim from enqueue — \
         story 35-15 closed the silent drop at `_image_model`. Got {:?}",
        calls[0].variant
    );
}

#[tokio::test]
async fn enqueue_with_lora_path_and_no_scale_forwards_none_scale() {
    // When the genre pack specifies a lora but no explicit scale, the
    // Rust side sends `lora_scale: None` — the daemon defaults to 1.0.
    // The worker must receive `None` here, NOT `Some(1.0)` (which would
    // be a silent default on the Rust side, violating the "no silent
    // fallbacks" rule — the Python daemon owns the default).
    let captures: Arc<Mutex<Vec<CapturedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let queue = make_capturing_queue(Arc::clone(&captures));

    let subject = make_subject("cave_entity");
    queue
        .enqueue(
            subject,
            "cave_painting",
            "dev",
            "",
            "",
            Some("/tmp/cave.safetensors"),
            None,
        )
        .await
        .expect("enqueue");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let calls = captures.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].lora_path.as_deref(),
        Some("/tmp/cave.safetensors")
    );
    assert!(
        calls[0].lora_scale.is_none(),
        "Rust side must not silently default lora_scale to 1.0 — that's \
         the daemon's job. Got {:?}",
        calls[0].lora_scale
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Wire/contract test — enqueue signature enforces Option, not String
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn enqueue_signature_accepts_none_lora_explicitly() {
    // STRENGTHENED per review finding #10 (2026-04-10). Previous version
    // used an always-Ok mock closure with `matches!(result, Ok(Queued))`
    // — the runtime assertion had zero discriminating power because the
    // mock always returns Ok regardless of whether `None` lora params
    // are forwarded correctly or silently dropped. The defensive comment
    // claiming "not vacuous" was wrong. This is the exact self-deception
    // Rule #6 of the lang-review checklist warns against.
    //
    // The fix: use a capturing closure that records whether `lora_path`
    // arrived as `None`, and assert on the capture. Now the test has a
    // real behavioral discriminant — if Dev accidentally converts None
    // to Some("") or Some("none") somewhere in the wire path, this test
    // catches it. The compile-time signature guard is preserved by the
    // explicit `None` arguments in the enqueue call.
    let captures: Arc<Mutex<Vec<CapturedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let queue = make_capturing_queue(Arc::clone(&captures));

    let subject = make_subject("none_test");
    let result = queue
        .enqueue(
            subject,
            "oil_painting",
            "", // variant — explicit empty (no override)
            "",
            "",
            None, // lora_path — explicit None
            None, // lora_scale — explicit None
        )
        .await;

    // Compile-time signature guard (Options preserved) — the call above
    // only compiles if enqueue's signature still accepts Option<_>.
    assert!(
        matches!(result, Ok(EnqueueResult::Queued { .. })),
        "enqueue with None lora params must succeed, got {result:?}"
    );

    // Behavioral assertion: the capturing closure must have received
    // `lora_path: None`, `lora_scale: None`, and an empty variant. This
    // is what gives the test discriminating power — if Dev accidentally
    // converts None to Some("") or Some("none") anywhere in the wire
    // path, this assertion catches it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let calls = captures.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "worker must have received exactly one call — got {}",
        calls.len()
    );
    assert!(
        calls[0].lora_path.is_none(),
        "worker must receive lora_path: None verbatim when None is passed \
         to enqueue, got {:?}",
        calls[0].lora_path
    );
    assert!(
        calls[0].lora_scale.is_none(),
        "worker must receive lora_scale: None verbatim when None is passed \
         to enqueue, got {:?}",
        calls[0].lora_scale
    );
    assert_eq!(
        calls[0].variant, "",
        "worker must receive empty variant verbatim, got {:?}",
        calls[0].variant
    );
}
