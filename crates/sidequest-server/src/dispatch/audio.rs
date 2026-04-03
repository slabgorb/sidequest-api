//! Audio/music processing — mood classification and cue generation.

use rand::Rng;
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
    location_changed: bool,
    combat_just_ended: bool,
) {
    if let Some(ref mut director) = ctx.music_director {
        tracing::info!("music_director_present — evaluating mood");
        let turn_number = ctx.turn_manager.interaction();
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
            // Story 12-1: cinematic variation context
            location_changed,
            scene_turn_count: if location_changed { 0 } else { turn_number as u32 },
            drama_weight: ctx.combat_state.drama_weight() as f32,
            combat_just_ended,
            session_start: turn_number <= 1,
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

        // Mood selection: MusicDirector's state-based classification is authoritative
        // (it checks in_combat, in_chase, quest_completed, etc.).  Narrator's
        // scene_mood overrides only if the narrator actually provided one —
        // most agents don't, so relying on it alone produces perpetual "exploration".
        let mood_key = match result.scene_mood.as_deref() {
            Some(mood) => {
                tracing::info!(mood = %mood, "music_mood_from_narrator — overriding classifier");
                mood
            }
            None => {
                let classified = mood_reasoning.classification.primary.as_key();
                tracing::info!(
                    mood = classified,
                    in_combat = mood_ctx.in_combat,
                    in_chase = mood_ctx.in_chase,
                    "music_mood_classified — narrator did not provide scene_mood"
                );
                classified
            }
        };

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
                // Story 12-1: variation telemetry from post-evaluate snapshot
                let post_telemetry = director.telemetry_snapshot();
                builder
                    .field("track_selected", &cue.track_id)
                    .field("variation", &post_telemetry.current_variation)
                    .field("variation_reason", &post_telemetry.variation_reason)
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

    // SFX triggers from narrator — resolve IDs to genre-prefixed file paths.
    // The narrator picks SFX IDs based on what happened in the scene.
    // The server resolves each ID to a random variant from the genre pack's sfx_library,
    // then prefixes with the genre path so the UI can fetch the file.
    if !result.sfx_triggers.is_empty() {
        let mut rng = rand::rng();
        let mut resolved_paths: Vec<String> = Vec::new();
        let mut invalid_ids: Vec<String> = Vec::new();

        for sfx_id in &result.sfx_triggers {
            if let Some(variants) = ctx.sfx_library.get(sfx_id.as_str()) {
                if !variants.is_empty() {
                    // Pick a random variant from the available files
                    let idx = rng.random_range(0..variants.len());
                    let path = &variants[idx];
                    // Prefix with genre path for client fetching
                    let full_path = format!("/genre/{}/{}", ctx.genre_slug, path);
                    resolved_paths.push(full_path);
                }
            } else {
                invalid_ids.push(sfx_id.clone());
            }
        }

        if !invalid_ids.is_empty() {
            tracing::warn!(
                invalid = ?invalid_ids,
                "sfx.invalid_ids — narrator emitted SFX IDs not in genre pack sfx_library"
            );
            WatcherEventBuilder::new("sfx", WatcherEventType::ValidationWarning)
                .field("action", "sfx_invalid_ids")
                .field("invalid_ids", &invalid_ids)
                .field("requested", &result.sfx_triggers)
                .send(ctx.state);
        }

        if !resolved_paths.is_empty() {
            tracing::info!(
                requested = ?result.sfx_triggers,
                resolved = ?resolved_paths,
                "sfx.triggers_resolved"
            );
            WatcherEventBuilder::new("sfx", WatcherEventType::StateTransition)
                .field("action", "sfx_triggered")
                .field("requested_ids", &result.sfx_triggers)
                .field("resolved_paths", &resolved_paths)
                .field("count", resolved_paths.len())
                .send(ctx.state);
            messages.push(GameMessage::AudioCue {
                payload: sidequest_protocol::AudioCuePayload {
                    mood: None,
                    music_track: None,
                    sfx_triggers: resolved_paths,
                    channel: Some("sfx".to_string()),
                    action: Some("play".to_string()),
                    volume: Some(0.7),
                },
                player_id: ctx.player_id.to_string(),
            });
        }
    }
}
