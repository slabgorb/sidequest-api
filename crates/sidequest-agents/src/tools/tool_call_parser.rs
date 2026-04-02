//! Tool call result parser — reads sidecar JSONL files produced by script tools
//! during Claude CLI execution and maps them to `ToolCallResults` for `assemble_turn`.
//!
//! ADR-057: Tool scripts write structured results to a session-specific sidecar file.
//! After the Claude CLI subprocess completes, the orchestrator reads the sidecar file,
//! parses each JSONL line into a `ToolCallRecord`, and maps known tools to the
//! corresponding `ToolCallResults` fields. The sidecar file is deleted after parsing.

use std::io::BufRead;
use std::path::PathBuf;

use tracing::{info, warn};

use crate::orchestrator::VisualScene;
use crate::tools::assemble_turn::ToolCallResults;

/// Directory where tool call sidecar files are written.
///
/// Tool scripts discover this via the `SIDEQUEST_TOOL_SIDECAR_DIR` environment variable,
/// falling back to this default. The orchestrator always reads from this path.
pub const SIDECAR_DIR: &str = "/tmp/sidequest-tools";

/// A single tool call record from the sidecar JSONL file.
///
/// Each line in the sidecar file is one `ToolCallRecord` serialized as JSON.
/// Tool scripts write these; the parser reads them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallRecord {
    /// The tool name (e.g., "set_mood", "set_intent").
    pub tool: String,
    /// The tool's result payload. Structure varies by tool.
    pub result: serde_json::Value,
}

/// Compute the sidecar file path for a given session ID.
pub fn sidecar_path(session_id: &str) -> PathBuf {
    PathBuf::from(SIDECAR_DIR).join(format!("sidequest-tools-{session_id}.jsonl"))
}

/// Parse tool call results from the sidecar JSONL file for a given session.
///
/// Returns `ToolCallResults` with fields populated from any recognized tool records.
/// If the sidecar file doesn't exist (no tools fired), returns default (all `None`).
/// Malformed lines are skipped with a warning. The sidecar file is deleted after parsing.
#[tracing::instrument(name = "tool_call_parser.parse", skip_all, fields(session_id = %session_id))]
pub fn parse_tool_results(session_id: &str) -> ToolCallResults {
    let path = sidecar_path(session_id);

    if !path.exists() {
        info!("no sidecar file — no tool calls fired this turn");
        return ToolCallResults::default();
    }

    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "failed to open sidecar file — returning default");
            return ToolCallResults::default();
        }
    };

    let reader = std::io::BufReader::new(file);
    let mut results = ToolCallResults::default();
    let mut parsed_count: usize = 0;
    let mut skipped_count: usize = 0;

    for (line_num, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warn!(line = line_num + 1, error = %e, "failed to read sidecar line — skipping");
                skipped_count += 1;
                continue;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let record: ToolCallRecord = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                warn!(line = line_num + 1, error = %e, "malformed JSONL line — skipping");
                skipped_count += 1;
                continue;
            }
        };

        match record.tool.as_str() {
            "set_mood" => {
                if let Some(mood) = record.result.get("mood").and_then(|v| v.as_str()) {
                    info!(tool = "set_mood", value = mood, "tool result parsed");
                    results.scene_mood = Some(mood.to_string());
                    parsed_count += 1;
                } else {
                    warn!(tool = "set_mood", "missing 'mood' field in result — skipping");
                    skipped_count += 1;
                }
            }
            "set_intent" => {
                if let Some(intent) = record.result.get("intent").and_then(|v| v.as_str()) {
                    info!(tool = "set_intent", value = intent, "tool result parsed");
                    results.scene_intent = Some(intent.to_string());
                    parsed_count += 1;
                } else {
                    warn!(tool = "set_intent", "missing 'intent' field in result — skipping");
                    skipped_count += 1;
                }
            }
            "scene_render" => {
                let subject = record.result.get("subject").and_then(|v| v.as_str());
                let tier = record.result.get("tier").and_then(|v| v.as_str());
                let mood = record.result.get("mood").and_then(|v| v.as_str());
                let tags = record.result.get("tags").and_then(|v| v.as_array());

                if let (Some(subject), Some(tier), Some(mood)) = (subject, tier, mood) {
                    let tag_strings: Vec<String> = tags
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    info!(
                        tool = "scene_render",
                        subject = subject,
                        tier = tier,
                        "tool result parsed"
                    );
                    results.visual_scene = Some(VisualScene {
                        subject: subject.to_string(),
                        tier: tier.to_string(),
                        mood: mood.to_string(),
                        tags: tag_strings,
                    });
                    parsed_count += 1;
                } else {
                    warn!(tool = "scene_render", "missing required fields (subject/tier/mood) in result — skipping");
                    skipped_count += 1;
                }
            }
            other => {
                warn!(tool = other, "unknown tool name — skipping");
                skipped_count += 1;
            }
        }
    }

    info!(parsed = parsed_count, skipped = skipped_count, "sidecar parsing complete");

    // Cleanup: delete sidecar file after parsing
    if let Err(e) = std::fs::remove_file(&path) {
        warn!(error = %e, "failed to delete sidecar file after parsing");
    }

    results
}
