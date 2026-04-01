//! Audio/music processing — mood classification and cue generation.

use sidequest_protocol::GameMessage;

use crate::extraction::audio_cue_to_game_message;
use crate::{WatcherEventBuilder, WatcherEventType};

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

        // OTEL: log encounter mood override if active
        if let Some(ref mood_override) = mood_ctx.encounter_mood_override {
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("action", "mood_override")
                .field("override_mood", mood_override)
                .send(ctx.state);
        }

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
            {
                let mut builder = WatcherEventBuilder::new("music_director", WatcherEventType::AgentSpanClose)
                    .field("turn_number", turn_approx)
                    .field("mood_classified", mood_reasoning.classification.primary.as_key())
                    .field("mood_reason", &mood_reasoning.reason)
                    .field("narrator_scene_mood", mood_key)
                    .field("intensity", mood_reasoning.classification.intensity)
                    .field("confidence", mood_reasoning.classification.confidence);
                if !mood_reasoning.keyword_matches.is_empty() {
                    builder = builder.field("keyword_matches",
                        mood_reasoning.keyword_matches.iter()
                            .map(|(mood, kw)| format!("{}:{}", mood, kw))
                            .collect::<Vec<_>>());
                }
                builder
                    .field("track_selected", &cue.track_id)
                    .field("previous_mood", &pre_telemetry.current_mood)
                    .field("previous_track", &pre_telemetry.current_track)
                    .field("action", cue.action.to_string())
                    .field("volume", cue.volume)
                    .field("rotation_history", &pre_telemetry.rotation_history)
                    .field("tracks_per_mood", &pre_telemetry.tracks_per_mood)
                    .send(ctx.state);
            }

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
            WatcherEventBuilder::new("music_director", WatcherEventType::AgentSpanClose)
                .field("turn_number", turn_approx)
                .field("mood_classified", mood_reasoning.classification.primary.as_key())
                .field("mood_reason", &mood_reasoning.reason)
                .field("narrator_scene_mood", mood_key)
                .field("suppressed", true)
                .field("suppression_reason", "same_mood_low_intensity")
                .field("current_mood", &pre_telemetry.current_mood)
                .field("current_track", &pre_telemetry.current_track)
                .send(ctx.state);
            tracing::warn!(
                mood = mood_key,
                "music_evaluate_returned_none — no cue produced"
            );
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }

    // SFX triggers from narrator — emitted as a separate AudioCue message.
    // The narrator picks SFX IDs based on what happened in the scene.
    if !result.sfx_triggers.is_empty() {
        // Validate: only emit SFX IDs that exist in the genre pack's sfx_library.
        let valid_sfx: Vec<String> = result
            .sfx_triggers
            .iter()
            .filter(|id| ctx.sfx_ids.contains(id))
            .cloned()
            .collect();

        if valid_sfx.len() < result.sfx_triggers.len() {
            let invalid: Vec<&String> = result
                .sfx_triggers
                .iter()
                .filter(|id| !ctx.sfx_ids.contains(id))
                .collect();
            tracing::warn!(
                invalid = ?invalid,
                "sfx.invalid_ids — narrator emitted SFX IDs not in genre pack sfx_library"
            );
        }

        if !valid_sfx.is_empty() {
            tracing::info!(
                sfx = ?valid_sfx,
                "sfx.triggers_emitted"
            );
            messages.push(GameMessage::AudioCue {
                payload: sidequest_protocol::AudioCuePayload {
                    mood: None,
                    music_track: None,
                    sfx_triggers: valid_sfx,
                    channel: Some("sfx".to_string()),
                    action: Some("play".to_string()),
                    volume: Some(0.7),
                },
                player_id: ctx.player_id.to_string(),
            });
        }
    }
}
