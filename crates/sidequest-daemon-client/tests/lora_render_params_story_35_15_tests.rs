//! RED phase tests for Story 35-15: Wire LoRA path from visual_style.yaml
//! through render pipeline to daemon.
//!
//! Covers AC-2: "render request to daemon includes lora_path when
//! visual_style has lora" — specifically the JSON serialization contract
//! between sidequest-daemon-client and the Python daemon.
//!
//! The daemon reads params via `params.get("lora_path")` at
//! `sidequest_daemon/media/workers/flux_mlx_worker.py:155`. The wire
//! contract is: if LoRA is active, the JSON must contain a top-level
//! `lora_path` string field; if LoRA is NOT active, the field must be
//! absent entirely (not `null`), so `params.get("lora_path")` returns
//! `None` on the daemon side.
//!
//! Default `lora_scale` is `1.0` on the daemon side
//! (flux_mlx_worker.py:156); the Rust side sends `Option<f32>` where
//! `None` means "let the daemon default it."

use sidequest_daemon_client::{build_request_json, RenderParams};

// ─────────────────────────────────────────────────────────────────────────
// AC-5 regression guardrail — non-LoRA renders must NOT include lora_path
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn default_render_params_has_no_lora_fields() {
    // REGRESSION TEST — written FIRST per SM assessment. This is the
    // load-bearing assertion: the default RenderParams (what non-LoRA
    // genres will produce) must not have `lora_path` or `lora_scale`.
    // If Dev accidentally removes `Option` or changes the default,
    // this catches it immediately.
    let params = RenderParams::default();
    assert!(
        params.lora_path.is_none(),
        "RenderParams::default() must have lora_path: None — \
         got {:?}. Non-LoRA genres must produce no LoRA field.",
        params.lora_path
    );
    assert!(
        params.lora_scale.is_none(),
        "RenderParams::default() must have lora_scale: None — \
         got {:?}. Non-LoRA genres must produce no LoRA field.",
        params.lora_scale
    );
}

#[test]
fn render_request_without_lora_omits_lora_path_from_json() {
    // REGRESSION TEST — this asserts `skip_serializing_if = "Option::is_none"`
    // is set. Without that attribute, the field would serialize as
    // `"lora_path":null`, which `params.get("lora_path")` on the Python
    // side treats as a present-but-null value (different from absent).
    //
    // The assertion is `.get("lora_path").is_none()` — which is TRUE
    // only if the key is *absent*, not if it's `null`. This is the
    // correct enforcement of the wire contract.
    let params = RenderParams::default();
    let json = build_request_json("render", &params);

    let params_obj = json["params"]
        .as_object()
        .expect("render params must serialize as a JSON object");

    assert!(
        !params_obj.contains_key("lora_path"),
        "Non-LoRA render must omit `lora_path` from JSON entirely. \
         Found: {}. Use #[serde(skip_serializing_if = \"Option::is_none\")].",
        serde_json::to_string(&params_obj).unwrap()
    );
    assert!(
        !params_obj.contains_key("lora_scale"),
        "Non-LoRA render must omit `lora_scale` from JSON entirely. \
         Found: {}. Use #[serde(skip_serializing_if = \"Option::is_none\")].",
        serde_json::to_string(&params_obj).unwrap()
    );
}

#[test]
fn non_lora_request_json_is_byte_identical_to_pre_35_15() {
    // Stronger regression guard: the JSON shape for a non-LoRA render
    // must be identical to the current pre-35-15 behavior. This catches
    // the class of bug where Dev adds `"lora_path":null` and thinks
    // that's fine because the daemon "handles both cases."
    //
    // The daemon DOES handle both cases today, but the wire contract
    // must be: absence means absence. Adding `null` bloats the request
    // size and breaks future parsers that rely on field presence as a
    // semantic signal.
    let params = RenderParams {
        prompt: "a forest path".to_string(),
        art_style: "oil_painting".to_string(),
        tier: "scene_illustration".to_string(),
        positive_prompt: "".to_string(),
        negative_prompt: "".to_string(),
        narration: "".to_string(),
        width: Some(768),
        height: Some(512),
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    // Every key that was present pre-35-15 is still present...
    assert!(params_obj.contains_key("prompt"));
    assert!(params_obj.contains_key("art_style"));
    assert!(params_obj.contains_key("tier"));
    assert!(params_obj.contains_key("width"));
    assert!(params_obj.contains_key("height"));
    // ...and the new LoRA keys are ABSENT.
    assert!(
        !params_obj.contains_key("lora_path"),
        "lora_path must be absent when not set"
    );
    assert!(
        !params_obj.contains_key("lora_scale"),
        "lora_scale must be absent when not set"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// AC-2 positive case — LoRA renders include the field
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn render_request_with_lora_path_includes_it_in_json() {
    let params = RenderParams {
        prompt: "a saloon confrontation".to_string(),
        art_style: "spaghetti western cinematography".to_string(),
        tier: "scene_illustration".to_string(),
        lora_path: Some(
            "/abs/path/to/genre_packs/spaghetti_western/lora/sw_style.safetensors".to_string(),
        ),
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    assert!(
        params_obj.contains_key("lora_path"),
        "lora_path must be present in JSON when Some(_). Full params: {}",
        serde_json::to_string(&params_obj).unwrap()
    );
    assert_eq!(
        params_obj["lora_path"].as_str(),
        Some("/abs/path/to/genre_packs/spaghetti_western/lora/sw_style.safetensors"),
        "lora_path JSON value must match the Rust-side value verbatim"
    );
}

#[test]
fn render_request_with_lora_scale_includes_it_in_json() {
    let params = RenderParams {
        prompt: "test".to_string(),
        art_style: "test".to_string(),
        tier: "portrait".to_string(),
        lora_path: Some("/tmp/test.safetensors".to_string()),
        lora_scale: Some(0.8),
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    assert!(
        params_obj.contains_key("lora_scale"),
        "lora_scale must be present in JSON when Some(_)"
    );
    // The daemon side reads this as a float and passes to Flux1 constructor
    // as `lora_scales=[lora_scale]`. Must arrive as a JSON number, not a
    // string.
    let scale = params_obj["lora_scale"]
        .as_f64()
        .expect("lora_scale must serialize as a JSON number, not a string");
    assert!(
        (scale - 0.8_f64).abs() < 1e-6,
        "lora_scale JSON value must match Rust value — got {scale}"
    );
}

#[test]
fn render_request_with_lora_path_but_no_scale_lets_daemon_default() {
    // Per daemon side (flux_mlx_worker.py:156), `lora_scale` defaults to
    // 1.0 if missing. The Rust side may ship `lora_path: Some, lora_scale:
    // None` when the genre pack doesn't override the scale. This must
    // serialize to a JSON with `lora_path` present and `lora_scale` absent
    // — NOT `lora_scale: null`, which would break future parsers.
    let params = RenderParams {
        prompt: "test".to_string(),
        art_style: "test".to_string(),
        tier: "portrait".to_string(),
        lora_path: Some("/tmp/test.safetensors".to_string()),
        lora_scale: None,
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    assert!(
        params_obj.contains_key("lora_path"),
        "lora_path must be present when Some"
    );
    assert!(
        !params_obj.contains_key("lora_scale"),
        "lora_scale must be ABSENT (not null) when None — \
         daemon defaults to 1.0 for missing key. Got: {}",
        serde_json::to_string(&params_obj).unwrap()
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Variant wire — the companion fix landed in story 35-15 alongside the
// LoRA wire. Previously `preferred_model` was read from YAML and silently
// dropped at the `_image_model` parameter in RenderQueue::enqueue().
// Now it's plumbed through `RenderParams.variant` to the daemon.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn default_render_params_has_empty_variant() {
    // REGRESSION: empty default means "let the daemon fall back to its
    // tier default." This is the no-override contract — no silent Rust
    // fallback to "dev" or "schnell".
    let params = RenderParams::default();
    assert_eq!(
        params.variant, "",
        "RenderParams::default().variant must be empty (no override). \
         Got {:?}. The daemon owns the tier-default fallback.",
        params.variant
    );
}

#[test]
fn render_request_with_empty_variant_omits_field_from_json() {
    // Parallel to the lora_path skip_serializing_if test: absence of
    // override MUST be absence of key, not `"variant":""` or
    // `"variant":null`. The daemon uses `params.get("variant", "")` and
    // `if requested_variant:` — present-but-empty would still fall through
    // to the tier default, but a buggy future daemon rewrite could treat
    // empty string as an invalid override. Absence is the safer contract.
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    assert!(
        !params_obj.contains_key("variant"),
        "Empty variant must be OMITTED from JSON (skip_serializing_if). \
         Found: {}",
        serde_json::to_string(&params_obj).unwrap()
    );
}

#[test]
fn render_request_with_variant_override_includes_it_in_json() {
    let params = RenderParams {
        prompt: "test".to_string(),
        art_style: "test".to_string(),
        tier: "portrait".to_string(),
        variant: "dev".to_string(),
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    let params_obj = json["params"].as_object().unwrap();

    assert!(
        params_obj.contains_key("variant"),
        "Non-empty variant must be PRESENT in JSON. Found: {}",
        serde_json::to_string(&params_obj).unwrap()
    );
    assert_eq!(
        params_obj["variant"].as_str(),
        Some("dev"),
        "variant JSON value must match the Rust-side value verbatim"
    );
}

#[test]
fn render_request_schnell_variant_survives_serialization() {
    // Both canonical variants must round-trip. The daemon validates
    // against {"dev", "schnell"} and raises loudly for anything else —
    // this test just proves Rust doesn't corrupt the value on the way out.
    let params = RenderParams {
        prompt: "test".to_string(),
        art_style: "test".to_string(),
        tier: "text_overlay".to_string(),
        variant: "schnell".to_string(),
        ..Default::default()
    };
    let json = build_request_json("render", &params);
    assert_eq!(json["params"]["variant"].as_str(), Some("schnell"));
}

#[test]
fn lora_path_survives_full_request_envelope() {
    // Verifies the outer JSON-RPC envelope doesn't strip the lora fields.
    // `build_request_json` wraps `params` inside `{id, method, params}`;
    // if the wrapping accidentally clones params through a restrictive
    // Serialize impl, lora_path could be dropped silently.
    let params = RenderParams {
        prompt: "test".to_string(),
        art_style: "test".to_string(),
        tier: "portrait".to_string(),
        lora_path: Some("/abs/test.safetensors".to_string()),
        lora_scale: Some(0.9),
        ..Default::default()
    };
    let json = build_request_json("render", &params);

    // Full envelope has id + method + params
    assert_eq!(json["method"], "render");
    assert!(json["id"].is_string());

    // Nested lora_path survives
    assert_eq!(
        json["params"]["lora_path"].as_str(),
        Some("/abs/test.safetensors")
    );
    assert!(
        (json["params"]["lora_scale"].as_f64().unwrap() - 0.9_f64).abs() < 1e-6
    );
}
