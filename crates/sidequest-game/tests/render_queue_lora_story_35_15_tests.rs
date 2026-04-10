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
    let captures_clone = Arc::clone(&captures);

    let queue = RenderQueue::spawn(
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
            let captures = Arc::clone(&captures_clone);
            async move {
                captures.lock().unwrap().push(CapturedCall {
                    prompt: prompt.clone(),
                    variant,
                    lora_path: lora_path.clone(),
                    lora_scale,
                });
                Ok((
                    format!("/tmp/mock/{prompt}.png"),
                    42u64,
                ))
            }
        },
    );

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
    let captures_clone = Arc::clone(&captures);

    let queue = RenderQueue::spawn(
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
            let captures = Arc::clone(&captures_clone);
            async move {
                captures.lock().unwrap().push(CapturedCall {
                    prompt: prompt.clone(),
                    variant,
                    lora_path,
                    lora_scale,
                });
                Ok((format!("/tmp/mock/{prompt}.png"), 42u64))
            }
        },
    );

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
    let captures_clone = Arc::clone(&captures);

    let queue = RenderQueue::spawn(
        test_config(),
        move |prompt: String,
              _: String,
              _: String,
              _: String,
              _: String,
              _: u32,
              _: u32,
              variant: String,
              lora_path: Option<String>,
              lora_scale: Option<f32>| {
            let captures = Arc::clone(&captures_clone);
            async move {
                captures.lock().unwrap().push(CapturedCall {
                    prompt,
                    variant,
                    lora_path,
                    lora_scale,
                });
                Ok(("ok".to_string(), 1u64))
            }
        },
    );

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
    // This test exists primarily as a compile-time guard: if Dev changes
    // the enqueue signature to `lora_path: &str` (non-optional), this
    // test fails to compile. The explicit `None` argument proves the
    // Option is preserved.
    //
    // The test is NOT vacuous — it asserts that calling with None
    // produces a successful enqueue result. Rule #6 of the Rust lang-
    // review checklist forbids `is_none()` on always-None values; here
    // we're asserting queue behavior, not the argument.
    let queue = RenderQueue::spawn(
        test_config(),
        |_: String,
         _: String,
         _: String,
         _: String,
         _: String,
         _: u32,
         _: u32,
         _: String,
         _: Option<String>,
         _: Option<f32>| async { Ok(("ok".to_string(), 0u64)) },
    );

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

    assert!(
        matches!(result, Ok(EnqueueResult::Queued { .. })),
        "enqueue with None lora params must succeed, got {result:?}"
    );
}
