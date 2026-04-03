//! Tests for agent infrastructure: Agent trait, ClaudeClient,
//! ContextBuilder, and format helpers.
//!
//! Story 1-10: Agent infrastructure — Agent trait, ClaudeClient,
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
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
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
            "Before responding, check...",
            AttentionZone::Recency,
            SectionCategory::Format,
        ));
        builder.add_section(PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
        builder.add_section(PromptSection::new(
            "state",
            "Current HP: 42",
            AttentionZone::Valley,
            SectionCategory::State,
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
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
        builder.add_section(PromptSection::new(
            "rules",
            "Never break character.",
            AttentionZone::Early,
            SectionCategory::Guardrail,
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
            "narrator identity",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
        builder.add_section(PromptSection::new(
            "rules",
            "guardrails",
            AttentionZone::Early,
            SectionCategory::Guardrail,
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
            "primacy content",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
        builder.add_section(PromptSection::new(
            "b",
            "valley content",
            AttentionZone::Valley,
            SectionCategory::State,
        ));
        builder.add_section(PromptSection::new(
            "c",
            "also primacy",
            AttentionZone::Primacy,
            SectionCategory::Guardrail,
        ));

        let primacy = builder.sections_by_zone(AttentionZone::Primacy);
        assert_eq!(primacy.len(), 2);
    }

    #[test]
    fn builder_token_estimate_sums_sections() {
        let mut builder = ContextBuilder::new();
        builder.add_section(PromptSection::new(
            "short",
            "one two three",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
        builder.add_section(PromptSection::new(
            "long",
            "four five six seven eight",
            AttentionZone::Valley,
            SectionCategory::State,
        ));

        let estimate = builder.token_estimate();
        assert_eq!(estimate, 8); // 3 + 5 words
    }
}

// Format helpers tests removed — format_helpers.rs deleted (superseded by
// inline formatting in sidequest-server::dispatch::prompt::build_prompt_context).
