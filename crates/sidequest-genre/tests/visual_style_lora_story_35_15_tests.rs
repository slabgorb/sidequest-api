//! RED phase tests for Story 35-15: Wire LoRA path from visual_style.yaml
//! through render pipeline to daemon.
//!
//! These tests cover AC-1: "visual_style.yaml with lora field is parsed
//! without error" — plus the architect's Design Deviation #1 (field name is
//! `lora_trigger`, not `trigger_word`) and the backwards-compatibility
//! guardrail (genres without the new fields must still parse).
//!
//! Per the architect's test order, AC-5 (regression guardrail — non-LoRA
//! genres continue to render normally) is the load-bearing test. Here it
//! takes the form of a deserialization test: VisualStyle YAML *without* the
//! new fields must still parse, with `lora` and `lora_trigger` defaulting
//! to `None`.

use sidequest_genre::VisualStyle;

// ─────────────────────────────────────────────────────────────────────────
// AC-5 regression guardrail — non-LoRA genres parse unchanged
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn visual_style_without_lora_fields_still_deserializes() {
    // REGRESSION TEST — written FIRST per SM assessment.
    // Most genres don't have trained LoRAs; silent breakage here would affect
    // every current playtest. This exact YAML shape is what ships in
    // sidequest-content today (e.g., low_fantasy/visual_style.yaml).
    let yaml = r#"
positive_suffix: oil painting, dramatic lighting
negative_prompt: blurry, low quality
preferred_model: schnell
base_seed: 42
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml)
        .expect("existing non-LoRA visual_style YAML must continue to deserialize");

    assert_eq!(style.preferred_model, "schnell");
    assert_eq!(style.base_seed, 42);

    // Both new fields must default to None when absent from YAML.
    // These assertions will fail at compile time until Dev adds the fields.
    assert!(
        style.lora.is_none(),
        "visual_style.yaml without a `lora` field must produce lora: None, got {:?}",
        style.lora
    );
    assert!(
        style.lora_trigger.is_none(),
        "visual_style.yaml without a `lora_trigger` field must produce lora_trigger: None, got {:?}",
        style.lora_trigger
    );
}

#[test]
fn visual_style_without_lora_fields_preserves_existing_behavior() {
    // Additional regression coverage — ensures the existing fields keep
    // their semantics when the new optional fields are absent. Written as
    // a separate test so a failure points at the affected field without
    // ambiguity.
    let yaml = r#"
positive_suffix: gritty post-apocalyptic digital painting
negative_prompt: clean, pristine
preferred_model: flux
base_seed: 7
visual_tag_overrides:
  wasteland: cracked sun-baked earth
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        style.positive_suffix,
        "gritty post-apocalyptic digital painting"
    );
    assert_eq!(style.negative_prompt, "clean, pristine");
    assert_eq!(style.preferred_model, "flux");
    assert_eq!(style.base_seed, 7);
    assert_eq!(style.visual_tag_overrides.len(), 1);
    assert_eq!(
        style
            .visual_tag_overrides
            .get("wasteland")
            .map(String::as_str),
        Some("cracked sun-baked earth")
    );
    // The new fields must still default to None even when other optional
    // fields ARE present — proves #[serde(default)] independence.
    assert!(style.lora.is_none());
    assert!(style.lora_trigger.is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// AC-1 — genres WITH lora fields deserialize correctly
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn visual_style_deserializes_with_lora_and_trigger() {
    // Canonical LoRA-enabled genre fixture. Per ADR-032 and the architect's
    // Design Deviation #1, the trigger field is `lora_trigger`, NOT
    // `trigger_word`. A test that used `trigger_word` here would silently
    // succeed (via `#[serde(default)]`) while leaving the trigger un-set,
    // so this test specifically asserts both fields are Some with the
    // expected values.
    let yaml = r#"
positive_suffix: spaghetti western cinematography, sergio leone style
negative_prompt: modern, clean, digital
preferred_model: dev
base_seed: 1898
lora: lora/spaghetti_western_style.safetensors
lora_trigger: sw_style
"#;
    let style: VisualStyle =
        serde_yaml::from_str(yaml).expect("visual_style.yaml with lora fields must deserialize");

    assert_eq!(
        style.lora.as_deref(),
        Some("lora/spaghetti_western_style.safetensors"),
        "lora field must round-trip the YAML string verbatim"
    );
    assert_eq!(
        style.lora_trigger.as_deref(),
        Some("sw_style"),
        "lora_trigger field must round-trip the YAML string verbatim"
    );
}

#[test]
fn visual_style_with_lora_but_no_trigger_deserializes() {
    // Edge case: a genre pack author forgets the trigger word. This must
    // still parse (both fields are independently optional), but the trigger
    // will be None — Dev's dispatch code must handle this case without
    // silent fallback (a LoRA without a trigger will load but do nothing
    // visually; the wiring code should log a warning).
    let yaml = r#"
positive_suffix: cave painting aesthetic
negative_prompt: modern
preferred_model: dev
base_seed: 100
lora: lora/cave_paintings.safetensors
"#;
    let style: VisualStyle =
        serde_yaml::from_str(yaml).expect("lora without lora_trigger must still parse");

    assert_eq!(
        style.lora.as_deref(),
        Some("lora/cave_paintings.safetensors")
    );
    assert!(
        style.lora_trigger.is_none(),
        "omitted lora_trigger must deserialize as None, not empty string"
    );
}

#[test]
fn visual_style_with_trigger_but_no_lora_deserializes() {
    // Inverse edge case: trigger without lora path. This is a YAML
    // authoring bug (trigger has no effect without a LoRA loaded), but
    // the deserializer must not reject it — the dispatch code is
    // responsible for validating the combination.
    let yaml = r#"
positive_suffix: painterly fantasy
negative_prompt: photographic
preferred_model: dev
base_seed: 1
lora_trigger: orphan_trigger
"#;
    let style: VisualStyle =
        serde_yaml::from_str(yaml).expect("lora_trigger without lora must still parse");

    assert!(style.lora.is_none());
    assert_eq!(style.lora_trigger.as_deref(), Some("orphan_trigger"));
}

// ─────────────────────────────────────────────────────────────────────────
// Architect's Design Deviation #1 — field name must be `lora_trigger`, not
// `trigger_word`. This test FAILS (trigger stays None) if Dev uses the
// wrong field name per the session file's original description.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn lora_trigger_field_is_named_lora_trigger_not_trigger_word() {
    // Per ADR-032 line 208 (`pub lora_trigger: Option<String>`) and the
    // architect's Design Deviation #1, the canonical field name is
    // `lora_trigger`. If Dev uses `trigger_word` instead, this YAML's
    // `trigger_word` key will silently be ignored (because #[serde(default)]
    // accepts missing fields) and `style.lora_trigger` will be None —
    // *which would also be None if the right field name were used and the
    // YAML used the wrong key*. This test distinguishes those cases by
    // using the correct key name and asserting Some(...).
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora: lora/test.safetensors
lora_trigger: test_trigger_value
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        style.lora_trigger.as_deref(),
        Some("test_trigger_value"),
        "Per ADR-032, the field name is `lora_trigger`. If this assertion \
         fails because lora_trigger is None, Dev used the wrong struct field name \
         (e.g., `trigger_word` per the stale session file description)."
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Round-trip serialization — proves serde(default) doesn't corrupt data
// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// REWORK Pass 2 (2026-04-10) — Finding A (reviewer self-correction)
//
// The first-pass review's finding #2 (add deny_unknown_fields to VisualStyle)
// was WRONG. It contradicts the pre-existing `visual_style_accepts_extra_fields`
// test in `tests/model_tests.rs:799` which intentionally documents VisualStyle
// as an exempt from the deny_unknown_fields convention — genre extensibility
// is a core feature. The two rejection tests written in Rework Pass 1
// (`visual_style_rejects_unknown_fields` and
// `visual_style_rejects_another_unknown_field`) have been DELETED per the
// architect's Rework Pass 2 assessment. See reviewer assessment Finding A.
// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// REWORK (2026-04-10) — Reviewer finding #3 (HIGH)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn visual_style_has_lora_scale_field() {
    // Reviewer finding #3 (HIGH): `lora_scale` is a dead wire in
    // production code today — it exists on `RenderParams`, threads
    // through `RenderJob` and the worker closure, and serializes
    // correctly, but **nothing sets it to a non-None value** in
    // production. VisualStyle has no `lora_scale` field;
    // dispatch/render.rs:206 hardcodes `None`. This is the same
    // dead-wire pattern as the `_image_model` parameter the user
    // explicitly rejected earlier in the story.
    //
    // RESOLUTION PATHS (Dev picks one):
    //
    // (a) WIRE IT: Add `lora_scale: Option<f32>` to VisualStyle with
    //     `#[serde(default)]`. Update dispatch/render.rs to read
    //     `vs.lora_scale` and pass it to enqueue. This test enforces
    //     path (a) — the test fails at compile time if Dev doesn't
    //     add the field, and passes at runtime once the field is
    //     wired through.
    //
    // (b) REMOVE IT: Remove `lora_scale` from RenderParams, RenderJob,
    //     the closure signature (back to 9 args from 10), and the
    //     daemon wire. If Dev picks path (b), **delete this test**
    //     along with the `lora_scale` tests in
    //     `lora_render_params_story_35_15_tests.rs`.
    //
    // Either resolution satisfies the "no half-wired features" rule.
    // The current state (dead wire in production) does not.
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora: lora/test.safetensors
lora_trigger: test
lora_scale: 0.75
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml).expect(
        "VisualStyle YAML with lora_scale must deserialize — Dev should add \
         `lora_scale: Option<f32>` with #[serde(default)] to the struct, \
         then wire it through dispatch/render.rs to RenderParams.lora_scale",
    );
    assert_eq!(
        style.lora_scale,
        Some(0.75),
        "VisualStyle must expose lora_scale as Option<f32> when provided \
         in the YAML. Per reviewer finding #3, either wire it through \
         dispatch/render.rs to close the dead wire, OR remove lora_scale \
         from RenderParams entirely and delete this test."
    );
}

#[test]
fn visual_style_without_lora_scale_defaults_to_none() {
    // Companion to `visual_style_has_lora_scale_field` — asserts
    // backward compatibility. Most genre packs don't specify a scale
    // (the daemon defaults to 1.0). An absent `lora_scale` in YAML
    // must deserialize as `None`, not as `Some(0.0)` or `Some(1.0)`.
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora: lora/test.safetensors
lora_trigger: test
"#;
    let style: VisualStyle = serde_yaml::from_str(yaml).unwrap();
    assert!(
        style.lora_scale.is_none(),
        "VisualStyle without `lora_scale` in YAML must deserialize as \
         None (daemon uses its 1.0 default). Got: {:?}",
        style.lora_scale
    );
}

// ─────────────────────────────────────────────────────────────────────────
// REWORK Pass 2 (2026-04-10) — Finding F: lora_scale validator
//
// `lora_scale` is a strength multiplier passed to Flux LoRA on the daemon.
// `serde_yaml` deserializes `.nan`, `.inf`, `-.inf` as valid f32 values
// with no validation, and the daemon behavior on non-finite LoRA scales is
// unspecified. These tests enforce the custom `validate_lora_scale`
// deserializer that rejects non-finite, negative, and out-of-range values
// at YAML-parse time so malformed configs never reach the daemon.
//
// Accepted range: [0.0, 2.0]. See `validate_lora_scale` doc comment for
// rationale on the upper bound.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn lora_scale_nan_yaml_fails_to_deserialize() {
    // Primary Finding F regression: `.nan` in YAML must be rejected.
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora_scale: .nan
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "lora_scale: .nan must be rejected by validate_lora_scale. Got: {result:?}"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("lora_scale") || err_msg.contains("finite"),
        "Error message must cite lora_scale or finiteness. Got: {err_msg}"
    );
}

#[test]
fn lora_scale_positive_infinity_yaml_fails_to_deserialize() {
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora_scale: .inf
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "lora_scale: .inf must be rejected by validate_lora_scale. Got: {result:?}"
    );
}

#[test]
fn lora_scale_negative_infinity_yaml_fails_to_deserialize() {
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora_scale: -.inf
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "lora_scale: -.inf must be rejected by validate_lora_scale. Got: {result:?}"
    );
}

#[test]
fn lora_scale_negative_value_yaml_fails_to_deserialize() {
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora_scale: -0.5
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "negative lora_scale must be rejected (no semantic meaning for negative LoRA strength). Got: {result:?}"
    );
}

#[test]
fn lora_scale_above_two_yaml_fails_to_deserialize() {
    // 2.0 is the canonical upper bound — values like `20` (typo of `2.0`)
    // must not silently reach the daemon.
    let yaml = r#"
positive_suffix: test
negative_prompt: test
preferred_model: dev
base_seed: 0
lora_scale: 20.0
"#;
    let result: Result<VisualStyle, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "lora_scale: 20.0 must be rejected (above 2.0 upper bound — likely a typo). Got: {result:?}"
    );
}

#[test]
fn lora_scale_boundary_values_deserialize() {
    // Happy-path boundaries: 0.0, 1.0 (daemon default), 2.0 (upper cap).
    for scale in ["0.0", "1.0", "2.0", "0.75"] {
        let yaml = format!(
            "positive_suffix: test\nnegative_prompt: test\npreferred_model: dev\nbase_seed: 0\nlora_scale: {scale}\n"
        );
        let style: VisualStyle = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("lora_scale: {scale} must deserialize cleanly, got {e}"));
        let expected: f32 = scale.parse().unwrap();
        assert_eq!(
            style.lora_scale,
            Some(expected),
            "lora_scale: {scale} must round-trip as Some({expected})"
        );
    }
}

#[test]
fn visual_style_roundtrips_through_serde() {
    // Deserialize → re-serialize → deserialize again must preserve
    // lora/lora_trigger values. This catches a class of bugs where
    // Dev accidentally adds `#[serde(skip)]` or misspells a serde
    // attribute and the round-trip silently drops the field.
    let original_yaml = r#"
positive_suffix: test suffix
negative_prompt: test negative
preferred_model: dev
base_seed: 123
lora: lora/roundtrip.safetensors
lora_trigger: rt_style
"#;
    let style1: VisualStyle = serde_yaml::from_str(original_yaml).unwrap();
    let yaml_out = serde_yaml::to_string(&style1).unwrap();
    let style2: VisualStyle = serde_yaml::from_str(&yaml_out).unwrap();

    assert_eq!(style1.lora, style2.lora);
    assert_eq!(style1.lora_trigger, style2.lora_trigger);
    assert_eq!(style1.positive_suffix, style2.positive_suffix);
    assert_eq!(style1.base_seed, style2.base_seed);
}
