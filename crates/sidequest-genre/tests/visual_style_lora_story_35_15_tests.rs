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
    assert_eq!(style.positive_suffix, "gritty post-apocalyptic digital painting");
    assert_eq!(style.negative_prompt, "clean, pristine");
    assert_eq!(style.preferred_model, "flux");
    assert_eq!(style.base_seed, 7);
    assert_eq!(style.visual_tag_overrides.len(), 1);
    assert_eq!(
        style.visual_tag_overrides.get("wasteland").map(String::as_str),
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
    let style: VisualStyle = serde_yaml::from_str(yaml)
        .expect("visual_style.yaml with lora fields must deserialize");

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
    let style: VisualStyle = serde_yaml::from_str(yaml)
        .expect("lora without lora_trigger must still parse");

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
    let style: VisualStyle = serde_yaml::from_str(yaml)
        .expect("lora_trigger without lora must still parse");

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
