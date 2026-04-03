//! Tool call result parser — reads sidecar JSONL files produced by script tools
//! during Claude CLI execution and maps them to `ToolCallResults` for `assemble_turn`.
//!
//! ADR-057: Tool scripts write structured results to a session-specific sidecar file.
//! After the Claude CLI subprocess completes, the orchestrator reads the sidecar file,
//! parses each JSONL line into a `ToolCallRecord`, and maps known tools to the
//! corresponding `ToolCallResults` fields. The sidecar file is deleted after parsing.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use tracing::{info, warn};

use crate::tools::assemble_turn::ToolCallResults;
use crate::tools::item_acquire::validate_item_acquire;
use crate::tools::merchant_transact::validate_merchant_transact;
use crate::tools::personality_event::validate_personality_event;
use crate::tools::scene_render::validate_scene_render;
use sidequest_game;

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
            "item_acquire" => {
                let item_ref = record.result.get("item_ref").and_then(|v| v.as_str());
                let name = record.result.get("name").and_then(|v| v.as_str());
                let category = record.result.get("category").and_then(|v| v.as_str());

                if let (Some(item_ref), Some(name), Some(category)) = (item_ref, name, category) {
                    match validate_item_acquire(item_ref, name, category) {
                        Ok(validated) => {
                            info!(
                                tool = "item_acquire",
                                item_ref = validated.item_ref(),
                                name = validated.name(),
                                category = validated.category(),
                                "tool result parsed"
                            );
                            let items = results.items_acquired.get_or_insert_with(Vec::new);
                            items.push(validated.to_item_gained());
                            parsed_count += 1;
                        }
                        Err(e) => {
                            warn!(tool = "item_acquire", error = %e, "item_acquire validation failed — skipping");
                            skipped_count += 1;
                        }
                    }
                } else {
                    warn!(tool = "item_acquire", "missing required fields (item_ref/name/category) in result — skipping");
                    skipped_count += 1;
                }
            }
            "merchant_transact" => {
                let transaction_type = record.result.get("transaction_type").and_then(|v| v.as_str());
                let item_id = record.result.get("item_id").and_then(|v| v.as_str());
                let merchant = record.result.get("merchant").and_then(|v| v.as_str());

                if let (Some(transaction_type), Some(item_id), Some(merchant)) = (transaction_type, item_id, merchant) {
                    match validate_merchant_transact(transaction_type, item_id, merchant) {
                        Ok(validated) => {
                            info!(
                                tool = "merchant_transact",
                                transaction_type = validated.transaction_type(),
                                item_id = validated.item_id(),
                                merchant = validated.merchant(),
                                "tool result parsed"
                            );
                            let txns = results.merchant_transactions.get_or_insert_with(Vec::new);
                            txns.push(validated.to_merchant_transaction_extracted());
                            parsed_count += 1;
                        }
                        Err(e) => {
                            warn!(tool = "merchant_transact", error = %e, "merchant_transact validation failed — skipping");
                            skipped_count += 1;
                        }
                    }
                } else {
                    warn!(tool = "merchant_transact", "missing required fields (transaction_type/item_id/merchant) in result — skipping");
                    skipped_count += 1;
                }
            }
            "scene_render" => {
                let subject = record.result.get("subject").and_then(|v| v.as_str());
                let tier = record.result.get("tier").and_then(|v| v.as_str());
                let mood = record.result.get("mood").and_then(|v| v.as_str());
                let tags = record.result.get("tags").and_then(|v| v.as_array());

                if let (Some(subject), Some(tier), Some(mood)) = (subject, tier, mood) {
                    let tag_refs: Vec<&str> = tags
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();

                    match validate_scene_render(subject, tier, mood, &tag_refs) {
                        Ok(scene) => {
                            info!(tool = "scene_render", subject = subject, tier = tier, "tool result parsed");
                            results.visual_scene = Some(scene);
                            parsed_count += 1;
                        }
                        Err(e) => {
                            warn!(tool = "scene_render", error = %e, "scene_render validation failed — skipping");
                            skipped_count += 1;
                        }
                    }
                } else {
                    warn!(tool = "scene_render", "missing required fields (subject/tier/mood) in result — skipping");
                    skipped_count += 1;
                }
            }
            "quest_update" => {
                let quest_name = record.result.get("quest_name").and_then(|v| v.as_str());
                let status = record.result.get("status").and_then(|v| v.as_str());

                if let (Some(quest_name), Some(status)) = (quest_name, status) {
                    match crate::tools::quest_update::validate_quest_update(quest_name, status) {
                        Ok(update) => {
                            info!(tool = "quest_update", quest_name = update.quest_name(), status = update.status(), "tool result parsed");
                            let map = results.quest_updates.get_or_insert_with(HashMap::new);
                            map.insert(update.quest_name().to_string(), update.status().to_string());
                            parsed_count += 1;
                        }
                        Err(e) => {
                            warn!(tool = "quest_update", error = %e, "quest_update validation failed — skipping");
                            skipped_count += 1;
                        }
                    }
                } else {
                    warn!(tool = "quest_update", "missing 'quest_name' or 'status' field in result — skipping");
                    skipped_count += 1;
                }
            }
            "personality_event" => {
                let npc = record.result.get("npc").and_then(|v| v.as_str());
                let event_type = record.result.get("event_type").and_then(|v| v.as_str());
                let description = record.result.get("description").and_then(|v| v.as_str()).unwrap_or("");

                if let (Some(npc), Some(event_type)) = (npc, event_type) {
                    match validate_personality_event(npc, event_type, description) {
                        Ok(validated) => {
                            // Convert to orchestrator PersonalityEvent
                            let game_event: sidequest_game::PersonalityEvent = serde_json::from_value(
                                serde_json::Value::String(validated.event_type_str().to_string())
                            ).unwrap(); // Safe: validate already ensured this is a valid variant
                            let pe = crate::orchestrator::PersonalityEvent {
                                npc: validated.npc().to_string(),
                                event_type: game_event,
                                description: validated.description().to_string(),
                            };
                            info!(tool = "personality_event", npc = validated.npc(), event_type = validated.event_type_str(), "tool result parsed");
                            let events = results.personality_events.get_or_insert_with(Vec::new);
                            events.push(pe);
                            parsed_count += 1;
                        }
                        Err(e) => {
                            warn!(tool = "personality_event", error = %e, "personality_event validation failed — skipping");
                            skipped_count += 1;
                        }
                    }
                } else {
                    warn!(tool = "personality_event", "missing 'npc' or 'event_type' field in result — skipping");
                    skipped_count += 1;
                }
            }
            "resource_change" => {
                let resource = record.result.get("resource").and_then(|v| v.as_str());
                let delta = record.result.get("delta").and_then(|v| v.as_f64());

                if let (Some(resource), Some(delta)) = (resource, delta) {
                    info!(tool = "resource_change", resource = resource, delta = delta, "tool result parsed");
                    let deltas = results.resource_deltas.get_or_insert_with(HashMap::new);
                    deltas.insert(resource.to_string(), delta);
                    parsed_count += 1;
                } else {
                    warn!(tool = "resource_change", "missing 'resource' or 'delta' field in result — skipping");
                    skipped_count += 1;
                }
            }
            "play_sfx" => {
                if let Some(sfx_id) = record.result.get("sfx_id").and_then(|v| v.as_str()) {
                    info!(tool = "play_sfx", sfx_id = sfx_id, "tool result parsed");
                    let triggers = results.sfx_triggers.get_or_insert_with(Vec::new);
                    triggers.push(sfx_id.to_string());
                    parsed_count += 1;
                } else {
                    warn!(tool = "play_sfx", "missing 'sfx_id' field in result — skipping");
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
