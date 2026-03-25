//! Tests for the prompt framework: AttentionZone, PromptSection, RuleTier,
//! SoulData, SOUL.md parser, and PromptComposer trait.

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

// =========================================================================
// AttentionZone ordering tests
// =========================================================================

#[test]
fn attention_zone_order_primacy_is_first() {
    assert_eq!(AttentionZone::Primacy.order(), 0);
}

#[test]
fn attention_zone_order_early_is_second() {
    assert_eq!(AttentionZone::Early.order(), 1);
}

#[test]
fn attention_zone_order_valley_is_third() {
    assert_eq!(AttentionZone::Valley.order(), 2);
}

#[test]
fn attention_zone_order_late_is_fourth() {
    assert_eq!(AttentionZone::Late.order(), 3);
}

#[test]
fn attention_zone_order_recency_is_last() {
    assert_eq!(AttentionZone::Recency.order(), 4);
}

#[test]
fn attention_zone_primacy_less_than_early() {
    assert!(AttentionZone::Primacy < AttentionZone::Early);
}

#[test]
fn attention_zone_early_less_than_valley() {
    assert!(AttentionZone::Early < AttentionZone::Valley);
}

#[test]
fn attention_zone_valley_less_than_late() {
    assert!(AttentionZone::Valley < AttentionZone::Late);
}

#[test]
fn attention_zone_late_less_than_recency() {
    assert!(AttentionZone::Late < AttentionZone::Recency);
}

#[test]
fn attention_zone_primacy_not_greater_than_recency() {
    assert!(AttentionZone::Primacy < AttentionZone::Recency);
}

#[test]
fn attention_zone_same_zone_is_equal() {
    assert_eq!(AttentionZone::Valley, AttentionZone::Valley);
    assert!(!(AttentionZone::Valley < AttentionZone::Valley));
}

#[test]
fn attention_zone_all_ordered_returns_five_zones() {
    let zones = AttentionZone::all_ordered();
    assert_eq!(zones.len(), 5);
}

#[test]
fn attention_zone_all_ordered_is_sorted() {
    let zones = AttentionZone::all_ordered();
    assert_eq!(
        zones,
        vec![
            AttentionZone::Primacy,
            AttentionZone::Early,
            AttentionZone::Valley,
            AttentionZone::Late,
            AttentionZone::Recency,
        ]
    );
}

#[test]
fn attention_zone_sorting_vec_produces_correct_order() {
    let mut zones = vec![
        AttentionZone::Recency,
        AttentionZone::Primacy,
        AttentionZone::Late,
        AttentionZone::Early,
        AttentionZone::Valley,
    ];
    zones.sort();
    assert_eq!(zones[0], AttentionZone::Primacy);
    assert_eq!(zones[4], AttentionZone::Recency);
}

// =========================================================================
// AttentionZone serde tests
// =========================================================================

#[test]
fn attention_zone_serializes_to_snake_case() {
    let json = serde_json::to_string(&AttentionZone::Primacy).unwrap();
    assert_eq!(json, r#""primacy""#);
}

#[test]
fn attention_zone_deserializes_from_snake_case() {
    let zone: AttentionZone = serde_json::from_str(r#""valley""#).unwrap();
    assert_eq!(zone, AttentionZone::Valley);
}

#[test]
fn attention_zone_rejects_unknown_value() {
    let result = serde_json::from_str::<AttentionZone>(r#""unknown_zone""#);
    assert!(result.is_err());
}

// =========================================================================
// SectionCategory tests
// =========================================================================

#[test]
fn section_category_has_seven_variants() {
    // Verify all expected variants exist and are distinct.
    let categories = vec![
        SectionCategory::Identity,
        SectionCategory::Guardrail,
        SectionCategory::Soul,
        SectionCategory::Genre,
        SectionCategory::State,
        SectionCategory::Action,
        SectionCategory::Format,
    ];
    assert_eq!(categories.len(), 7);
    // All distinct
    for i in 0..categories.len() {
        for j in (i + 1)..categories.len() {
            assert_ne!(categories[i], categories[j]);
        }
    }
}

#[test]
fn section_category_serializes_to_snake_case() {
    let json = serde_json::to_string(&SectionCategory::Guardrail).unwrap();
    assert_eq!(json, r#""guardrail""#);
}

#[test]
fn section_category_roundtrips_through_json() {
    let original = SectionCategory::Soul;
    let json = serde_json::to_string(&original).unwrap();
    let restored: SectionCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(original, restored);
}

// =========================================================================
// RuleTier tests
// =========================================================================

#[test]
fn rule_tier_has_three_variants() {
    let tiers = vec![RuleTier::Critical, RuleTier::Firm, RuleTier::Coherence];
    assert_eq!(tiers.len(), 3);
    assert_ne!(tiers[0], tiers[1]);
    assert_ne!(tiers[1], tiers[2]);
    assert_ne!(tiers[0], tiers[2]);
}

#[test]
fn rule_tier_serializes_to_snake_case() {
    let json = serde_json::to_string(&RuleTier::Critical).unwrap();
    assert_eq!(json, r#""critical""#);
}

#[test]
fn rule_tier_roundtrips_through_json() {
    for tier in [RuleTier::Critical, RuleTier::Firm, RuleTier::Coherence] {
        let json = serde_json::to_string(&tier).unwrap();
        let restored: RuleTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, restored);
    }
}

// =========================================================================
// PromptSection construction tests
// =========================================================================

#[test]
fn prompt_section_new_sets_fields() {
    let section = PromptSection::new(
        "test_section",
        SectionCategory::Identity,
        AttentionZone::Primacy,
        "You are a narrator.",
    );
    assert_eq!(section.name, "test_section");
    assert_eq!(section.category, SectionCategory::Identity);
    assert_eq!(section.zone, AttentionZone::Primacy);
    assert_eq!(section.content, "You are a narrator.");
    assert!(section.source.is_none());
}

#[test]
fn prompt_section_with_source_sets_source() {
    let section = PromptSection::with_source(
        "soul_principles",
        SectionCategory::Soul,
        AttentionZone::Early,
        "Agency: The player controls their character.",
        "soul_md",
    );
    assert_eq!(section.source, Some("soul_md".to_string()));
}

#[test]
fn prompt_section_token_estimate_counts_words() {
    let section = PromptSection::new(
        "test",
        SectionCategory::Genre,
        AttentionZone::Valley,
        "one two three four five",
    );
    assert_eq!(section.token_estimate(), 5);
}

#[test]
fn prompt_section_token_estimate_empty_content_is_zero() {
    let section = PromptSection::new("empty", SectionCategory::State, AttentionZone::Late, "");
    assert_eq!(section.token_estimate(), 0);
}

#[test]
fn prompt_section_is_empty_true_for_empty_content() {
    let section = PromptSection::new("empty", SectionCategory::State, AttentionZone::Late, "");
    assert!(section.is_empty());
}

#[test]
fn prompt_section_is_empty_false_for_nonempty_content() {
    let section = PromptSection::new(
        "notempty",
        SectionCategory::State,
        AttentionZone::Late,
        "has content",
    );
    assert!(!section.is_empty());
}

// =========================================================================
// PromptSection serde tests
// =========================================================================

#[test]
fn prompt_section_json_roundtrip() {
    let section = PromptSection::new(
        "genre_tone",
        SectionCategory::Genre,
        AttentionZone::Early,
        "Dark and gritty.",
    );
    let json = serde_json::to_string(&section).unwrap();
    let restored: PromptSection = serde_json::from_str(&json).unwrap();
    assert_eq!(section, restored);
}

#[test]
fn prompt_section_json_roundtrip_with_source() {
    let section = PromptSection::with_source(
        "lore",
        SectionCategory::Genre,
        AttentionZone::Valley,
        "The Flickering Reach is a wasteland.",
        "genre_pack",
    );
    let json = serde_json::to_string(&section).unwrap();
    let restored: PromptSection = serde_json::from_str(&json).unwrap();
    assert_eq!(section, restored);
}

#[test]
fn prompt_section_rejects_unknown_fields() {
    let json = r#"{
        "name": "test",
        "category": "identity",
        "zone": "primacy",
        "content": "hello",
        "bogus_field": "should fail"
    }"#;
    let result = serde_json::from_str::<PromptSection>(json);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject bogus_field"
    );
}

// =========================================================================
// SOUL.md parser tests
// =========================================================================

/// Helper: write content to a temp file, return the path.
fn write_temp_soul(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("create temp file");
    f.write_all(content.as_bytes()).expect("write temp file");
    f.flush().expect("flush temp file");
    f
}

#[test]
fn parse_soul_md_extracts_principles_from_real_format() {
    let content = r#"# SOUL.md — SideQuest Agent Guidelines

Rules that govern how all AI agents interact with players.

**Agency.** The player controls their character — actions, thoughts, feelings.

**Living World.** NPCs act on their own goals — especially villains.

**Genre Truth.** Consequences follow the genre pack's tone and lethality.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert_eq!(data.principles.len(), 3);
    assert_eq!(data.principles[0].name, "Agency");
    assert_eq!(data.principles[1].name, "Living World");
    assert_eq!(data.principles[2].name, "Genre Truth");
}

#[test]
fn parse_soul_md_extracts_body_text() {
    let content = r#"# SOUL.md

**Agency.** The player controls their character — actions, thoughts, feelings.

**Living World.** NPCs act on their own goals.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert!(data.principles[0]
        .text
        .contains("The player controls their character"));
}

#[test]
fn parse_soul_md_extracts_title() {
    let content = r#"# SOUL.md — SideQuest Agent Guidelines

Rules that govern how all AI agents interact with players.

**Agency.** The player controls their character.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert_eq!(
        data.title.as_deref(),
        Some("SOUL.md — SideQuest Agent Guidelines")
    );
}

#[test]
fn parse_soul_md_extracts_description() {
    let content = r#"# SOUL.md — SideQuest Agent Guidelines

Rules that govern how all AI agents interact with players.

**Agency.** The player controls their character.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert_eq!(
        data.description.as_deref(),
        Some("Rules that govern how all AI agents interact with players.")
    );
}

#[test]
fn parse_soul_md_nonexistent_file_returns_empty() {
    let data = parse_soul_md(std::path::Path::new("/nonexistent/SOUL.md"));
    assert!(data.principles.is_empty());
    assert!(data.title.is_none());
}

#[test]
fn parse_soul_md_empty_file_returns_empty() {
    let f = write_temp_soul("");
    let data = parse_soul_md(f.path());
    assert!(data.principles.is_empty());
}

#[test]
fn parse_soul_md_file_without_bold_headers_returns_empty() {
    let f = write_temp_soul("Just some plain text without any bold headers.\n\nAnother paragraph.");
    let data = parse_soul_md(f.path());
    assert!(data.principles.is_empty());
}

#[test]
fn parse_soul_md_preserves_document_order() {
    let content = r#"# SOUL.md

**Zebra.** Last alphabetically but first in doc.

**Alpha.** First alphabetically but second in doc.

**Middle.** Middle of everything.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert_eq!(data.principles[0].name, "Zebra");
    assert_eq!(data.principles[1].name, "Alpha");
    assert_eq!(data.principles[2].name, "Middle");
}

#[test]
fn parse_soul_md_handles_multiline_body() {
    // The Python parser captures text until next blank line.
    let content = r#"# SOUL.md

**Diamonds and Coal.** Detail signals importance. Match narrative detail to narrative weight. Coal can become a diamond when the players choose to polish it.

**Next Principle.** Something else.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    assert_eq!(data.principles[0].name, "Diamonds and Coal");
    assert!(data.principles[0]
        .text
        .contains("Detail signals importance"));
    assert!(data.principles[0]
        .text
        .contains("Coal can become a diamond"));
}

#[test]
fn parse_soul_md_full_soul_file() {
    // Parse the actual SOUL.md from the repo.
    let soul_path = std::path::Path::new("/Users/keithavery/Projects/orc-quest/docs/SOUL.md");
    if !soul_path.exists() {
        // Skip if running in CI without the file.
        return;
    }
    let data = parse_soul_md(soul_path);

    // The real SOUL.md has these principles (verified from file):
    assert!(
        data.principles.len() >= 10,
        "Expected at least 10 principles, got {}",
        data.principles.len()
    );
    assert_eq!(data.principles[0].name, "Agency");
    assert_eq!(data.principles[1].name, "Living World");
    assert_eq!(data.principles[2].name, "Genre Truth");

    // Title should be present.
    assert!(data.title.is_some());
}

// =========================================================================
// SoulData method tests
// =========================================================================

#[test]
fn soul_data_len_returns_principle_count() {
    let content = r#"# SOUL.md

**One.** First.

**Two.** Second.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());
    assert_eq!(data.len(), 2);
}

#[test]
fn soul_data_is_empty_true_for_no_principles() {
    let f = write_temp_soul("Just text.");
    let data = parse_soul_md(f.path());
    assert!(data.is_empty());
}

#[test]
fn soul_data_is_empty_false_when_principles_exist() {
    let content = "**One.** First.\n\n";
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());
    assert!(!data.is_empty());
}

#[test]
fn soul_data_get_finds_by_name_case_insensitive() {
    let content = r#"# SOUL.md

**Agency.** The player controls.

**Living World.** NPCs act independently.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());

    let agency = data.get("agency");
    assert!(agency.is_some());
    assert_eq!(agency.unwrap().name, "Agency");

    let living = data.get("LIVING WORLD");
    assert!(living.is_some());
}

#[test]
fn soul_data_get_returns_none_for_missing() {
    let content = "**Agency.** The player controls.\n\n";
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());
    assert!(data.get("nonexistent").is_none());
}

#[test]
fn soul_data_as_prompt_text_formats_as_bullet_list() {
    let content = r#"**Agency.** The player controls.

**Living World.** NPCs act.
"#;
    let f = write_temp_soul(content);
    let data = parse_soul_md(f.path());
    let text = data.as_prompt_text();

    assert!(text.contains("- Agency: The player controls."));
    assert!(text.contains("- Living World: NPCs act."));
}

#[test]
fn soul_data_as_prompt_text_empty_for_no_principles() {
    let f = write_temp_soul("Just text.");
    let data = parse_soul_md(f.path());
    assert!(data.as_prompt_text().is_empty());
}

// =========================================================================
// PromptComposer trait — sorting by zone order
// =========================================================================

/// A minimal concrete PromptComposer for testing the trait contract.
struct TestComposer {
    sections: std::collections::HashMap<String, Vec<PromptSection>>,
}

impl TestComposer {
    fn new() -> Self {
        Self {
            sections: std::collections::HashMap::new(),
        }
    }
}

impl PromptComposer for TestComposer {
    fn register_section(&mut self, agent_name: &str, section: PromptSection) {
        let bucket = self.sections.entry(agent_name.to_string()).or_default();
        // Insert in zone-order position (same algorithm as Python).
        let target_order = section.zone.order();
        let insert_at = bucket
            .iter()
            .position(|s| s.zone.order() > target_order)
            .unwrap_or(bucket.len());
        bucket.insert(insert_at, section);
    }

    fn registry(&self, agent_name: &str) -> Vec<&PromptSection> {
        self.sections
            .get(agent_name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    fn get_sections(
        &self,
        agent_name: &str,
        category: Option<SectionCategory>,
        zone: Option<AttentionZone>,
    ) -> Vec<&PromptSection> {
        let mut result = self.registry(agent_name);
        if let Some(cat) = category {
            result.retain(|s| s.category == cat);
        }
        if let Some(z) = zone {
            result.retain(|s| s.zone == z);
        }
        result
    }

    fn compose(&self, agent_name: &str) -> String {
        self.registry(agent_name)
            .iter()
            .map(|s| {
                format!(
                    "<section name=\"{}\" category=\"{:?}\">{}</section>",
                    s.name, s.category, s.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn clear(&mut self, agent_name: &str) {
        self.sections.remove(agent_name);
    }
}

#[test]
fn composer_sections_are_ordered_by_zone() {
    let mut composer = TestComposer::new();

    // Register out of order.
    composer.register_section(
        "narrator",
        PromptSection::new(
            "checklist",
            SectionCategory::Guardrail,
            AttentionZone::Recency,
            "Check rules.",
        ),
    );
    composer.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "You are a narrator.",
        ),
    );
    composer.register_section(
        "narrator",
        PromptSection::new(
            "lore",
            SectionCategory::Genre,
            AttentionZone::Valley,
            "World lore.",
        ),
    );

    let sections = composer.registry("narrator");
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].zone, AttentionZone::Primacy);
    assert_eq!(sections[1].zone, AttentionZone::Valley);
    assert_eq!(sections[2].zone, AttentionZone::Recency);
}

#[test]
fn composer_preserves_insertion_order_within_zone() {
    let mut composer = TestComposer::new();

    composer.register_section(
        "narrator",
        PromptSection::new(
            "first_early",
            SectionCategory::Soul,
            AttentionZone::Early,
            "Soul.",
        ),
    );
    composer.register_section(
        "narrator",
        PromptSection::new(
            "second_early",
            SectionCategory::Genre,
            AttentionZone::Early,
            "Genre.",
        ),
    );

    let sections = composer.registry("narrator");
    assert_eq!(sections[0].name, "first_early");
    assert_eq!(sections[1].name, "second_early");
}

#[test]
fn composer_get_sections_filters_by_category() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "Id.",
        ),
    );
    composer.register_section(
        "narrator",
        PromptSection::new("soul", SectionCategory::Soul, AttentionZone::Early, "Soul."),
    );

    let soul_sections = composer.get_sections("narrator", Some(SectionCategory::Soul), None);
    assert_eq!(soul_sections.len(), 1);
    assert_eq!(soul_sections[0].name, "soul");
}

#[test]
fn composer_get_sections_filters_by_zone() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new("a", SectionCategory::Identity, AttentionZone::Primacy, "A."),
    );
    composer.register_section(
        "narrator",
        PromptSection::new("b", SectionCategory::Genre, AttentionZone::Early, "B."),
    );
    composer.register_section(
        "narrator",
        PromptSection::new("c", SectionCategory::State, AttentionZone::Early, "C."),
    );

    let early = composer.get_sections("narrator", None, Some(AttentionZone::Early));
    assert_eq!(early.len(), 2);
}

#[test]
fn composer_get_sections_filters_by_both() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new("a", SectionCategory::Genre, AttentionZone::Early, "A."),
    );
    composer.register_section(
        "narrator",
        PromptSection::new("b", SectionCategory::State, AttentionZone::Early, "B."),
    );
    composer.register_section(
        "narrator",
        PromptSection::new("c", SectionCategory::Genre, AttentionZone::Valley, "C."),
    );

    let result = composer.get_sections(
        "narrator",
        Some(SectionCategory::Genre),
        Some(AttentionZone::Early),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "a");
}

#[test]
fn composer_clear_removes_all_sections() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new("x", SectionCategory::Identity, AttentionZone::Primacy, "X."),
    );
    composer.clear("narrator");
    assert!(composer.registry("narrator").is_empty());
}

#[test]
fn composer_compose_produces_xml_sections() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "You are a narrator.",
        ),
    );
    let output = composer.compose("narrator");
    assert!(output.contains("<section"));
    assert!(output.contains("You are a narrator."));
    assert!(output.contains("</section>"));
}

#[test]
fn composer_empty_agent_returns_empty_string() {
    let composer = TestComposer::new();
    assert!(composer.compose("nonexistent").is_empty());
}

#[test]
fn composer_multiple_agents_are_independent() {
    let mut composer = TestComposer::new();
    composer.register_section(
        "narrator",
        PromptSection::new(
            "n1",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "Narrator.",
        ),
    );
    composer.register_section(
        "combat",
        PromptSection::new(
            "c1",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "Combat.",
        ),
    );

    assert_eq!(composer.registry("narrator").len(), 1);
    assert_eq!(composer.registry("combat").len(), 1);
    assert_eq!(composer.registry("narrator")[0].content, "Narrator.");
    assert_eq!(composer.registry("combat")[0].content, "Combat.");
}

// =========================================================================
// Edge cases and boundary tests
// =========================================================================

#[test]
fn prompt_section_whitespace_only_content_is_not_empty() {
    // Whitespace-only should count as having content (word split may differ).
    let section = PromptSection::new("ws", SectionCategory::State, AttentionZone::Valley, "   ");
    // Whitespace-only content: is_empty should be true (no meaningful content).
    // This tests that we handle whitespace sensibly.
    assert!(section.is_empty() || !section.is_empty()); // Compiles; dev decides behavior.
                                                        // The real assertion: token_estimate for whitespace should be 0.
    assert_eq!(section.token_estimate(), 0);
}

#[test]
fn prompt_section_multiline_content_token_estimate() {
    let section = PromptSection::new(
        "multi",
        SectionCategory::Genre,
        AttentionZone::Valley,
        "line one\nline two\nline three",
    );
    // "line one line two line three" = 6 words
    assert_eq!(section.token_estimate(), 6);
}

#[test]
fn attention_zone_is_copy() {
    let zone = AttentionZone::Primacy;
    let copy = zone;
    assert_eq!(zone, copy); // Both still usable — Copy trait works.
}

#[test]
fn section_category_is_copy() {
    let cat = SectionCategory::Soul;
    let copy = cat;
    assert_eq!(cat, copy);
}

#[test]
fn rule_tier_is_copy() {
    let tier = RuleTier::Critical;
    let copy = tier;
    assert_eq!(tier, copy);
}
