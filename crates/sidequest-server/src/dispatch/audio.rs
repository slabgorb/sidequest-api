//! Audio/music processing — mood classification and cue generation.

use std::collections::HashMap;

use sidequest_protocol::GameMessage;

use crate::extraction::audio_cue_to_game_message;
use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Audio/music — use narrator's scene_mood, or fall back to MusicDirector classification.
#[tracing::instrument(name = "turn.audio", skip_all)]
pub(crate) async fn process_audio(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    messages: &mut Vec<GameMessage>,
    result: &sidequest_agents::orchestrator::ActionResult,
) {
    if let Some(ref mut director) = ctx.music_director {
        tracing::info!("music_director_present — evaluating mood");
        let mood_ctx = sidequest_game::MoodContext {
            in_combat: ctx.combat_state.in_combat(),
            in_chase: ctx.chase_state.is_some(),
            party_health_pct: if *ctx.max_hp > 0 {
                *ctx.hp as f32 / *ctx.max_hp as f32
            } else {
                1.0
            },
            // Quest completion and NPC death now come from structured quest_updates,
            // not keyword scanning. Check if any quest was marked "completed:".
            quest_completed: result.quest_updates.values().any(|v| v.starts_with("completed")),
            npc_died: ctx.npc_registry.iter().any(|n| n.max_hp > 0 && n.hp <= 0),
            // Encounter mood override — derive from live state, read mood_override if set
            encounter_mood_override: {
                let encounter = if ctx.combat_state.in_combat() {
                    Some(sidequest_game::StructuredEncounter::from_combat_state(ctx.combat_state))
                } else {
                    ctx.chase_state.as_ref().map(sidequest_game::StructuredEncounter::from_chase_state)
                };
                encounter.and_then(|e| e.mood_override)
            },
        };

        // Get telemetry snapshot BEFORE evaluate() changes state
        let pre_telemetry = director.telemetry_snapshot();
        let mood_reasoning = director.classify_mood_with_reasoning(clean_narration, &mood_ctx);

        // Use narrator's scene_mood — it's required every turn.
        let mood_key = match result.scene_mood.as_deref() {
            Some(mood) => {
                tracing::info!(mood = %mood, "music_mood_from_narrator");
                mood
            }
            None => {
                tracing::error!("narrator did not provide scene_mood — defaulting to exploration");
                "exploration"
            }
        };
        tracing::info!(
            mood = mood_key,
            in_combat = mood_ctx.in_combat,
            "music_mood_classified"
        );

        // Get turn_number for watcher event (approximate from turn_manager)
        let turn_approx = ctx.turn_manager.interaction();

        if let Some(cue) = director.evaluate(clean_narration, &mood_ctx) {
            tracing::info!(
                mood = mood_key,
                track = ?cue.track_id,
                ctx.action = %cue.action,
                volume = cue.volume,
                "music_cue_produced"
            );

            // Emit rich music telemetry to watcher
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "music_director".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                    f.insert("mood_classified".to_string(), serde_json::json!(mood_reasoning.classification.primary.as_key()));
                    f.insert("mood_reason".to_string(), serde_json::json!(mood_reasoning.reason));
                    f.insert("narrator_scene_mood".to_string(), serde_json::json!(mood_key));
                    f.insert("intensity".to_string(), serde_json::json!(mood_reasoning.classification.intensity));
                    f.insert("confidence".to_string(), serde_json::json!(mood_reasoning.classification.confidence));
                    if !mood_reasoning.keyword_matches.is_empty() {
                        f.insert("keyword_matches".to_string(), serde_json::json!(
                            mood_reasoning.keyword_matches.iter()
                                .map(|(mood, kw)| format!("{}:{}", mood, kw))
                                .collect::<Vec<_>>()
                        ));
                    }
                    f.insert("track_selected".to_string(), serde_json::json!(cue.track_id));
                    f.insert("previous_mood".to_string(), serde_json::json!(pre_telemetry.current_mood));
                    f.insert("previous_track".to_string(), serde_json::json!(pre_telemetry.current_track));
                    f.insert("action".to_string(), serde_json::json!(cue.action.to_string()));
                    f.insert("volume".to_string(), serde_json::json!(cue.volume));
                    f.insert("rotation_history".to_string(), serde_json::json!(pre_telemetry.rotation_history));
                    f.insert("tracks_per_mood".to_string(), serde_json::json!(pre_telemetry.tracks_per_mood));
                    f
                },
            });

            let mixer_cues = {
                let mut mixer_guard = ctx.audio_mixer.lock().await;
                if let Some(ref mut mixer) = *mixer_guard {
                    mixer.apply_cue(cue)
                } else {
                    vec![cue]
                }
            };
            tracing::info!(cue_count = mixer_cues.len(), "music_mixer_cues_ready");
            for c in &mixer_cues {
                messages.push(audio_cue_to_game_message(
                    c,
                    ctx.player_id,
                    ctx.genre_slug,
                    Some(mood_key),
                ));
            }
        } else {
            // Mood didn't change — still emit telemetry so dashboard shows suppression
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "music_director".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                    f.insert("mood_classified".to_string(), serde_json::json!(mood_reasoning.classification.primary.as_key()));
                    f.insert("mood_reason".to_string(), serde_json::json!(mood_reasoning.reason));
                    f.insert("narrator_scene_mood".to_string(), serde_json::json!(mood_key));
                    f.insert("suppressed".to_string(), serde_json::json!(true));
                    f.insert("suppression_reason".to_string(), serde_json::json!("same_mood_low_intensity"));
                    f.insert("current_mood".to_string(), serde_json::json!(pre_telemetry.current_mood));
                    f.insert("current_track".to_string(), serde_json::json!(pre_telemetry.current_track));
                    f
                },
            });
            tracing::warn!(
                mood = mood_key,
                "music_evaluate_returned_none — no cue produced"
            );
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }
}
