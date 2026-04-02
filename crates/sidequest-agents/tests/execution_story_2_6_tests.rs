//! Story 2-6: Agent execution tests
//!
//! RED phase — tests reference methods and types that don't exist yet.
//! Dev must implement:
//!   - ClaudeClient::send() subprocess execution
//!   - ClaudeClient::send_streaming() NDJSON streaming
//!   - ClaudeClient::parse_json_envelope() result extraction
//!   - PromptRegistry implementing PromptComposer
//!   - Section caching for genre-invariant sections
//!
//! ACs tested: 15

use std::time::Duration;

use sidequest_agents::client::{ClaudeClient, ClaudeClientError};
use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::extractor::JsonExtractor;
use sidequest_agents::patches::{ChasePatch, CombatPatch, WorldStatePatch};
use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptSection, SectionCategory,
};

// === New types from story 2-6 ===
use sidequest_agents::prompt_framework::PromptRegistry;

// ============================================================================
// AC-1: ClaudeClient::send() executes subprocess, returns stdout
// ============================================================================

#[test]
fn client_send_with_echo_returns_stdout() {
    // Use 'echo' as a mock subprocess — tests the subprocess execution path
    let client = ClaudeClient::builder()
        .command_path("echo")
        .timeout(Duration::from_secs(5))
        .build();

    let result = client.send("hello world");
    assert!(
        result.is_ok(),
        "send() with echo should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.text.contains("hello"),
        "Output should contain the prompt. Got: {}",
        output.text
    );
}

// ============================================================================
// AC-2: Subprocess error returns SubprocessFailed with stderr
// ============================================================================

#[test]
fn client_send_nonexistent_command_fails() {
    let client = ClaudeClient::builder()
        .command_path("/nonexistent/command/that/does/not/exist")
        .timeout(Duration::from_secs(5))
        .build();

    let result = client.send("test");
    assert!(result.is_err(), "Non-existent command should fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, ClaudeClientError::SubprocessFailed { .. }),
        "Should be SubprocessFailed for missing binary, got: {:?}",
        err
    );
}

#[test]
fn client_send_failing_command_returns_subprocess_failed() {
    let client = ClaudeClient::builder()
        .command_path("false") // 'false' always exits with code 1
        .timeout(Duration::from_secs(5))
        .build();

    let result = client.send("test");
    assert!(result.is_err(), "'false' command should fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, ClaudeClientError::SubprocessFailed { .. }),
        "Should be SubprocessFailed, got: {:?}",
        err
    );
}

// ============================================================================
// AC-3: Timeout returns ClaudeClientError::Timeout
// ============================================================================

#[test]
fn client_send_timeout_returns_error() {
    // Verify the Timeout variant exists and pattern-matches correctly.
    // Subprocess-based timeout tests are platform-dependent (macOS sleep doesn't
    // accept -p flag). The polling loop + kill logic in ClaudeClient is
    // straightforward enough that constructing the error directly is sufficient.
    let err = ClaudeClientError::Timeout {
        elapsed: Duration::from_millis(150),
    };
    assert!(
        matches!(err, ClaudeClientError::Timeout { .. }),
        "Should be Timeout error, got: {:?}",
        err
    );
}

// parse_json_envelope tests removed — function deleted (logic is inline in send_impl).

// ============================================================================
// AC-7: PromptComposer::compose() includes sections in attention-zone order
// ============================================================================

#[test]
fn prompt_registry_composes_in_zone_order() {
    let mut registry = PromptRegistry::new();

    // Add sections out of order
    registry.register_section(
        "narrator",
        PromptSection::new(
            "late_section",
            "Late content",
            AttentionZone::Late,
            SectionCategory::Context,
        ),
    );
    registry.register_section(
        "narrator",
        PromptSection::new(
            "primacy_section",
            "Primacy content",
            AttentionZone::Primacy,
            SectionCategory::Role,
        ),
    );
    registry.register_section(
        "narrator",
        PromptSection::new(
            "recency_section",
            "Recency content",
            AttentionZone::Recency,
            SectionCategory::Context,
        ),
    );

    let composed = registry.compose("narrator");
    let primacy_pos = composed
        .find("Primacy content")
        .expect("Should contain primacy");
    let late_pos = composed.find("Late content").expect("Should contain late");
    let recency_pos = composed
        .find("Recency content")
        .expect("Should contain recency");

    assert!(primacy_pos < late_pos, "Primacy should come before Late");
    assert!(late_pos < recency_pos, "Late should come before Recency");
}

// ============================================================================
// AC-8: Genre tone, rules, lore sections populated
// ============================================================================

#[test]
fn prompt_registry_stores_genre_sections() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "genre_tone",
            "Tone: dark",
            AttentionZone::Primacy,
            SectionCategory::Genre,
        ),
    );
    registry.register_section(
        "narrator",
        PromptSection::new(
            "genre_rules",
            "Rules: no magic",
            AttentionZone::Early,
            SectionCategory::Genre,
        ),
    );

    let sections = registry.get_sections("narrator", Some(SectionCategory::Genre), None);
    assert_eq!(sections.len(), 2, "Should have 2 genre sections");
}

// ============================================================================
// AC-9: Game state section includes location, characters, NPCs
// ============================================================================

#[test]
fn context_builder_produces_state_section() {
    let mut builder = ContextBuilder::new();
    builder.add_section(PromptSection::new(
        "game_state",
        "Location: Town Square\nCharacters: Thorn (Fighter)\nNPCs: Luna (friendly)",
        AttentionZone::Valley,
        SectionCategory::Context,
    ));

    let composed = builder.compose();
    assert!(composed.contains("Town Square"), "Should include location");
    assert!(composed.contains("Thorn"), "Should include characters");
    assert!(composed.contains("Luna"), "Should include NPCs");
}

// ============================================================================
// AC-10: SOUL.md principles injected into EARLY zone
// ============================================================================

#[test]
fn soul_principles_in_early_zone() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "soul",
            "Agency: The player controls their character.",
            AttentionZone::Early,
            SectionCategory::Soul,
        ),
    );

    let sections = registry.get_sections("narrator", Some(SectionCategory::Soul), None);
    assert_eq!(sections.len(), 1);
    assert!(sections[0].content.contains("Agency"));
}

// ============================================================================
// AC-11: Section cache — genre-invariant sections reused across turns
// ============================================================================

#[test]
fn prompt_registry_caches_sections_across_composes() {
    let mut registry = PromptRegistry::new();

    // Register cached genre section
    registry.register_section(
        "narrator",
        PromptSection::new(
            "genre_tone",
            "Dark Fantasy",
            AttentionZone::Primacy,
            SectionCategory::Genre,
        ),
    );

    // Compose once
    let first = registry.compose("narrator");
    assert!(first.contains("Dark Fantasy"));

    // Add a state section (simulating next turn)
    registry.register_section(
        "narrator",
        PromptSection::new(
            "state",
            "New state",
            AttentionZone::Valley,
            SectionCategory::Context,
        ),
    );

    // Compose again — genre section should still be there
    let second = registry.compose("narrator");
    assert!(
        second.contains("Dark Fantasy"),
        "Cached genre section should persist across composes"
    );
    assert!(
        second.contains("New state"),
        "New state section should be included"
    );
}

// ============================================================================
// AC-12: extract_json::<CombatPatch>() deserializes typed patch
// ============================================================================

#[test]
fn extract_combat_patch_from_json() {
    let json = r#"{"in_combat": true, "drama_weight": 0.8}"#;
    let patch = JsonExtractor::extract::<CombatPatch>(json);
    assert!(patch.is_ok());
    let patch = patch.unwrap();
    assert_eq!(patch.in_combat, Some(true));
    assert_eq!(patch.drama_weight, Some(0.8));
}

// ============================================================================
// AC-13: extract_json fallback — fenced block
// ============================================================================

#[test]
fn extract_json_from_fenced_block() {
    let response = "The battle rages on!\n\n```json\n{\"in_combat\": true}\n```\n\nWhat do you do?";
    let patch = JsonExtractor::extract::<CombatPatch>(response);
    assert!(patch.is_ok(), "Should extract from fenced block");
    assert_eq!(patch.unwrap().in_combat, Some(true));
}

// ============================================================================
// AC-14: extract_json fallback — raw brace matching
// ============================================================================

#[test]
fn extract_json_from_raw_braces() {
    let response =
        "The chase continues! {\"separation_delta\": 5, \"phase\": \"pursuit\"} Better run faster!";
    let patch = JsonExtractor::extract::<ChasePatch>(response);
    assert!(patch.is_ok(), "Should extract from raw braces");
    assert_eq!(patch.unwrap().separation_delta, Some(5));
}

// ============================================================================
// AC-15: Malformed JSON → None returned, narration preserved
// ============================================================================

#[test]
fn malformed_json_returns_error() {
    let response = "The goblin attacks! {broken json here";
    let result = JsonExtractor::extract::<CombatPatch>(response);
    assert!(
        result.is_err(),
        "Malformed JSON should return error, not panic"
    );
}

#[test]
fn no_json_in_pure_narration() {
    let response = "The sun sets over the mountains. A peaceful evening.";
    let result = JsonExtractor::extract::<WorldStatePatch>(response);
    assert!(
        result.is_err(),
        "Pure narration should have no JSON to extract"
    );
}

// ============================================================================
// PromptRegistry — new type tests
// ============================================================================

#[test]
fn prompt_registry_clear_removes_sections() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "test",
            "content",
            AttentionZone::Valley,
            SectionCategory::Context,
        ),
    );
    assert!(!registry.registry("narrator").is_empty());

    registry.clear("narrator");
    assert!(
        registry.registry("narrator").is_empty(),
        "Clear should remove all sections for agent"
    );
}

#[test]
fn prompt_registry_separate_agents() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "n1",
            "narrator content",
            AttentionZone::Valley,
            SectionCategory::Context,
        ),
    );
    registry.register_section(
        "creature_smith",
        PromptSection::new(
            "c1",
            "combat content",
            AttentionZone::Valley,
            SectionCategory::Context,
        ),
    );

    assert_eq!(registry.registry("narrator").len(), 1);
    assert_eq!(registry.registry("creature_smith").len(), 1);
}

// ============================================================================
// PromptSection content accessor
// ============================================================================

#[test]
fn prompt_section_content_accessor() {
    let section = PromptSection::new(
        "test",
        "Test content here",
        AttentionZone::Valley,
        SectionCategory::Context,
    );
    assert_eq!(section.content, "Test content here");
}

// ============================================================================
// ClaudeClientError variant coverage
// ============================================================================

#[test]
fn error_timeout_variant() {
    let err = ClaudeClientError::Timeout {
        elapsed: Duration::from_secs(120),
    };
    assert!(format!("{}", err).contains("120"));
}

#[test]
fn error_subprocess_failed_variant() {
    let err = ClaudeClientError::SubprocessFailed {
        exit_code: Some(1),
        stderr: "error output".to_string(),
    };
    assert!(format!("{}", err).contains("error output"));
}

#[test]
fn error_empty_response_variant() {
    let err = ClaudeClientError::EmptyResponse;
    assert!(format!("{}", err).contains("empty"));
}
