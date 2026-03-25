//! Tests for agent infrastructure: Agent trait, ClaudeClient, JsonExtractor,
//! ContextBuilder, and format helpers.
//!
//! Story 1-10: Agent infrastructure — Agent trait, ClaudeClient, JsonExtractor,
//! ContextBuilder, format helpers.
//!
//! These are TDD RED tests — they reference types and modules that don't exist yet.
//! Dev implements the minimum code to make them pass.

// ═══════════════════════════════════════════════════════════
// Agent trait — port lesson #7
// ═══════════════════════════════════════════════════════════

mod agent_trait {
    use sidequest_agents::agent::{Agent, AgentResponse};

    /// A minimal test agent for verifying the trait contract.
    struct TestAgent;

    impl Agent for TestAgent {
        fn name(&self) -> &str {
            "test_agent"
        }

        fn system_prompt(&self) -> &str {
            "You are a test agent."
        }
    }

    #[test]
    fn agent_has_name() {
        let agent = TestAgent;
        assert_eq!(agent.name(), "test_agent");
    }

    #[test]
    fn agent_has_system_prompt() {
        let agent = TestAgent;
        assert_eq!(agent.system_prompt(), "You are a test agent.");
    }

    #[test]
    fn agent_response_has_text_field() {
        let response = AgentResponse {
            text: "Hello world".to_string(),
            raw_output: "Hello world".to_string(),
        };
        assert_eq!(response.text, "Hello world");
    }

    #[test]
    fn agent_response_has_raw_output() {
        let response = AgentResponse {
            text: "parsed".to_string(),
            raw_output: "```json\n{}\n```\nparsed".to_string(),
        };
        assert_ne!(response.text, response.raw_output);
    }
}

// ═══════════════════════════════════════════════════════════
// ClaudeClient — port lesson #3 (single subprocess wrapper)
// ═══════════════════════════════════════════════════════════

mod claude_client {
    use sidequest_agents::client::{ClaudeClient, ClaudeClientError};
    use std::time::Duration;

    #[test]
    fn client_default_timeout_is_120s() {
        let client = ClaudeClient::new();
        assert_eq!(client.timeout(), Duration::from_secs(120));
    }

    #[test]
    fn client_custom_timeout() {
        let client = ClaudeClient::with_timeout(Duration::from_secs(30));
        assert_eq!(client.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn client_default_command_is_claude() {
        let client = ClaudeClient::new();
        assert_eq!(client.command_path(), "claude");
    }

    #[test]
    fn client_custom_command_path() {
        let client = ClaudeClient::builder()
            .command_path("/usr/local/bin/claude")
            .build();
        assert_eq!(client.command_path(), "/usr/local/bin/claude");
    }

    #[test]
    fn client_builder_sets_timeout_and_path() {
        let client = ClaudeClient::builder()
            .timeout(Duration::from_secs(60))
            .command_path("/opt/claude")
            .build();
        assert_eq!(client.timeout(), Duration::from_secs(60));
        assert_eq!(client.command_path(), "/opt/claude");
    }

    // Error type tests — rule #1 (no silent error swallowing)
    #[test]
    fn error_timeout_is_distinct_variant() {
        let err = ClaudeClientError::Timeout {
            elapsed: Duration::from_secs(120),
        };
        assert!(matches!(err, ClaudeClientError::Timeout { .. }));
    }

    #[test]
    fn error_subprocess_failure_carries_exit_code() {
        let err = ClaudeClientError::SubprocessFailed {
            exit_code: Some(1),
            stderr: "error message".to_string(),
        };
        match err {
            ClaudeClientError::SubprocessFailed { exit_code, stderr } => {
                assert_eq!(exit_code, Some(1));
                assert_eq!(stderr, "error message");
            }
            _ => panic!("expected SubprocessFailed"),
        }
    }

    #[test]
    fn error_empty_response_is_distinct() {
        let err = ClaudeClientError::EmptyResponse;
        assert!(matches!(err, ClaudeClientError::EmptyResponse));
    }

    // Rule #2: error enum should be #[non_exhaustive]
    // (Verified structurally — the enum exists with a wildcard arm needed)

    #[test]
    fn error_implements_std_error() {
        let err = ClaudeClientError::EmptyResponse;
        // std::error::Error is implemented (thiserror)
        let _msg = format!("{err}");
        assert!(!_msg.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════
// JsonExtractor — port lesson #2 (single 3-tier extraction)
// ═══════════════════════════════════════════════════════════

mod json_extractor {
    use sidequest_agents::extractor::{ExtractionError, JsonExtractor};

    // Tier 1: Direct JSON parse
    #[test]
    fn extract_direct_json_object() {
        let input = r#"{"action": "move", "direction": "north"}"#;
        let result: serde_json::Value = JsonExtractor::extract(input).unwrap();
        assert_eq!(result["action"], "move");
        assert_eq!(result["direction"], "north");
    }

    #[test]
    fn extract_direct_json_array() {
        let input = r#"[{"name": "sword"}, {"name": "shield"}]"#;
        let result: serde_json::Value = JsonExtractor::extract(input).unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    // Tier 2: Markdown fence extraction
    #[test]
    fn extract_from_json_fence() {
        let input = "Here is the response:\n```json\n{\"hp\": 42}\n```\nDone.";
        let result: serde_json::Value = JsonExtractor::extract(input).unwrap();
        assert_eq!(result["hp"], 42);
    }

    #[test]
    fn extract_from_bare_fence() {
        let input = "Response:\n```\n{\"status\": \"ok\"}\n```";
        let result: serde_json::Value = JsonExtractor::extract(input).unwrap();
        assert_eq!(result["status"], "ok");
    }

    // Tier 3: Freeform search (find JSON in mixed text)
    #[test]
    fn extract_freeform_json_in_prose() {
        let input = "The narrator says: I think {\"mood\": \"tense\", \"location\": \"cave\"} is the state.";
        let result: serde_json::Value = JsonExtractor::extract(input).unwrap();
        assert_eq!(result["mood"], "tense");
    }

    // Failure cases
    #[test]
    fn extract_fails_on_no_json() {
        let input = "This is just plain text with no JSON at all.";
        let result = JsonExtractor::extract::<serde_json::Value>(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ExtractionError::NoJsonFound => {}
            other => panic!("expected NoJsonFound, got: {other:?}"),
        }
    }

    #[test]
    fn extract_fails_on_invalid_json_in_fence() {
        let input = "```json\n{invalid json here}\n```";
        let result = JsonExtractor::extract::<serde_json::Value>(input);
        assert!(result.is_err());
        match result.unwrap_err() {
            ExtractionError::ParseFailed { .. } => {}
            other => panic!("expected ParseFailed, got: {other:?}"),
        }
    }

    // Typed extraction
    #[test]
    fn extract_into_typed_struct() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct MoveAction {
            action: String,
            direction: String,
        }

        let input = r#"{"action": "move", "direction": "north"}"#;
        let result: MoveAction = JsonExtractor::extract(input).unwrap();
        assert_eq!(result.action, "move");
        assert_eq!(result.direction, "north");
    }

    #[test]
    fn extract_rejects_wrong_typed_struct() {
        #[derive(serde::Deserialize, Debug)]
        struct ExpectedType {
            #[allow(dead_code)]
            required_field: i32,
        }

        let input = r#"{"other_field": "text"}"#;
        let result = JsonExtractor::extract::<ExpectedType>(input);
        assert!(result.is_err());
    }

    // Rule #2: ExtractionError should be #[non_exhaustive]
    #[test]
    fn extraction_error_implements_std_error() {
        let err = ExtractionError::NoJsonFound;
        let msg = format!("{err}");
        assert!(!msg.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════
// ContextBuilder — port lesson #8 (composable sections)
// ═══════════════════════════════════════════════════════════

mod context_builder {
    use sidequest_agents::context_builder::ContextBuilder;
    use sidequest_agents::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

    #[test]
    fn empty_builder_produces_empty_context() {
        let builder = ContextBuilder::new();
        let sections = builder.build();
        assert!(sections.is_empty());
    }

    #[test]
    fn builder_adds_section() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "You are the narrator.",
        ));
        let sections = builder.build();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "identity");
    }

    #[test]
    fn builder_orders_sections_by_attention_zone() {
        let mut builder = ContextBuilder::new();
        // Add in reverse order
        builder.add_section(PromptSection::new(
            "checklist",
            SectionCategory::Format,
            AttentionZone::Recency,
            "Before responding, check...",
        ));
        builder.add_section(PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "You are the narrator.",
        ));
        builder.add_section(PromptSection::new(
            "state",
            SectionCategory::State,
            AttentionZone::Valley,
            "Current HP: 42",
        ));

        let sections = builder.build();
        assert_eq!(sections.len(), 3);
        // Should be in zone order: Primacy, Valley, Recency
        assert_eq!(sections[0].zone, AttentionZone::Primacy);
        assert_eq!(sections[1].zone, AttentionZone::Valley);
        assert_eq!(sections[2].zone, AttentionZone::Recency);
    }

    #[test]
    fn builder_composes_to_string() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "You are the narrator.",
        ));
        builder.add_section(PromptSection::new(
            "rules",
            SectionCategory::Guardrail,
            AttentionZone::Early,
            "Never break character.",
        ));

        let text = builder.compose();
        assert!(text.contains("You are the narrator."));
        assert!(text.contains("Never break character."));
        // Primacy content should come before Early content
        let narrator_pos = text.find("You are the narrator.").unwrap();
        let rules_pos = text.find("Never break character.").unwrap();
        assert!(narrator_pos < rules_pos, "Primacy should precede Early");
    }

    #[test]
    fn builder_filters_by_category() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "narrator identity",
        ));
        builder.add_section(PromptSection::new(
            "rules",
            SectionCategory::Guardrail,
            AttentionZone::Early,
            "guardrails",
        ));

        let identity_sections = builder.sections_by_category(SectionCategory::Identity);
        assert_eq!(identity_sections.len(), 1);
        assert_eq!(identity_sections[0].name, "identity");
    }

    #[test]
    fn builder_filters_by_zone() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "a",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "primacy content",
        ));
        builder.add_section(PromptSection::new(
            "b",
            SectionCategory::State,
            AttentionZone::Valley,
            "valley content",
        ));
        builder.add_section(PromptSection::new(
            "c",
            SectionCategory::Guardrail,
            AttentionZone::Primacy,
            "also primacy",
        ));

        let primacy = builder.sections_by_zone(AttentionZone::Primacy);
        assert_eq!(primacy.len(), 2);
    }

    #[test]
    fn builder_token_estimate_sums_sections() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "short",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            "one two three",
        ));
        builder.add_section(PromptSection::new(
            "long",
            SectionCategory::State,
            AttentionZone::Valley,
            "four five six seven eight",
        ));

        let estimate = builder.token_estimate();
        assert_eq!(estimate, 8); // 3 + 5 words
    }
}

// ═══════════════════════════════════════════════════════════
// Format helpers — ported from Python format_helpers.py
// ═══════════════════════════════════════════════════════════

mod format_helpers {
    use sidequest_agents::format_helpers;

    #[test]
    fn format_character_block_includes_name_and_hp() {
        let block = format_helpers::character_block("Kira", 42, 50, 5);
        assert!(block.contains("Kira"));
        assert!(block.contains("42"));
        assert!(block.contains("50"));
    }

    #[test]
    fn format_character_block_includes_level() {
        let block = format_helpers::character_block("Kira", 42, 50, 5);
        assert!(block.contains("5"));
    }

    #[test]
    fn format_location_block_includes_region_and_area() {
        let block = format_helpers::location_block("Flickering Reach", "Tood's Dome");
        assert!(block.contains("Flickering Reach"));
        assert!(block.contains("Tood's Dome"));
    }

    #[test]
    fn format_npc_block_includes_name_and_attitude() {
        let block = format_helpers::npc_block("Griztok", "hostile");
        assert!(block.contains("Griztok"));
        assert!(block.contains("hostile"));
    }

    #[test]
    fn format_inventory_summary_lists_items() {
        let items = vec!["Rusty Sword".to_string(), "Health Potion".to_string()];
        let summary = format_helpers::inventory_summary(&items);
        assert!(summary.contains("Rusty Sword"));
        assert!(summary.contains("Health Potion"));
    }

    #[test]
    fn format_inventory_summary_empty_says_no_items() {
        let items: Vec<String> = vec![];
        let summary = format_helpers::inventory_summary(&items);
        assert!(
            summary.to_lowercase().contains("no items") || summary.to_lowercase().contains("empty"),
            "empty inventory should indicate no items"
        );
    }
}
