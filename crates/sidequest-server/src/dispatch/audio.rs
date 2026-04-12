//! Audio/music processing — mood classification and cue generation.

use rand::Rng;
use sidequest_protocol::GameMessage;

use crate::extraction::{audio_cue_to_game_message, extract_location_header};
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
    encounter_just_resolved: bool,
) {
    let in_combat = ctx.in_combat();
    let in_chase = ctx.in_chase();
    let encounter_mood = ctx
        .snapshot
        .encounter
        .as_ref()
        .and_then(|e| e.mood_override.clone());
    if let Some(ref mut director) = ctx.music_director {
        tracing::info!("music_director_present — evaluating mood");
        let turn_number = ctx.turn_manager.interaction();
        let mood_ctx = sidequest_game::MoodContext {
            in_combat,
            in_chase,
            party_health_pct: if *ctx.max_hp > 0 {
                *ctx.hp as f32 / *ctx.max_hp as f32
            } else {
                1.0
            },
            quest_completed: result
                .quest_updates
                .values()
                .any(|v| v.starts_with("completed")),
            npc_died: ctx.npc_registry.iter().any(|n| n.max_hp > 0 && n.hp <= 0),
            // Encounter mood override — read directly from snapshot encounter (story 28-9)
            encounter_mood_override: encounter_mood,
            // Story 12-1: cinematic variation context
            location_changed,
            scene_turn_count: if location_changed {
                0
            } else {
                turn_number as u32
            },
            drama_weight: 0.0, // drama_weight removed with CombatState (story 28-9)
            combat_just_ended: encounter_just_resolved,
            session_start: turn_number <= 1,
        };

        // OTEL: log encounter mood override if active
        if let Some(ref mood_override) = mood_ctx.encounter_mood_override {
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("action", "mood_override")
                .field("override_mood", mood_override)
                .send();
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
                let classified = mood_reasoning.classification.primary.as_str();
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

        // Epic 16: Faction-aware music routing
        // Location faction: look up controlled_by from cartography regions for current location.
        let location_faction = if !ctx.snapshot.location.is_empty() {
            use sidequest_genre::GenreCode;
            GenreCode::new(ctx.genre_slug)
                .ok()
                .and_then(|gc| {
                    ctx.state
                        .genre_cache()
                        .get_or_load(&gc, ctx.state.genre_loader())
                        .ok()
                })
                .and_then(|pack| pack.worlds.get(ctx.world_slug).cloned())
                .and_then(|world| {
                    // Match by region name (case-insensitive) against cartography regions
                    let loc_lower = ctx.snapshot.location.to_lowercase();
                    world
                        .cartography
                        .regions
                        .values()
                        .find(|r| r.name.to_lowercase() == loc_lower)
                        .and_then(|r| r.controlled_by.clone())
                })
        } else {
            None
        };

        if let Some(ref faction) = location_faction {
            tracing::info!(faction = %faction, location = %ctx.snapshot.location, "faction.location_resolved");
        }

        // NPC faction field not yet on NpcRegistryEntry — actor_factions remains empty
        // until the NPC model tracks faction membership.
        let faction_ctx = sidequest_game::FactionContext {
            location_faction,
            actor_factions: vec![],
            player_reputation: None,
        };

        // Use faction-aware evaluation when faction themes exist in the genre's audio config
        let has_factions = !director.faction_themes_empty();
        let eval_result = if has_factions {
            director.evaluate_with_faction(clean_narration, &mood_ctx, &faction_ctx)
        } else {
            director.evaluate(clean_narration, &mood_ctx)
        };

        match eval_result {
            sidequest_game::MusicEvalResult::Cue(cue) => {
                tracing::info!(
                    mood = mood_key,
                    track = ?cue.track_id,
                    ctx.action = %cue.action,
                    volume = cue.volume,
                    "music_cue_produced"
                );

                // Emit rich music telemetry to watcher
                {
                    let mut builder = WatcherEventBuilder::new(
                        "music_director",
                        WatcherEventType::AgentSpanClose,
                    )
                    .field("turn_number", turn_approx)
                    .field(
                        "mood_classified",
                        mood_reasoning.classification.primary.as_str(),
                    )
                    .field("mood_reason", &mood_reasoning.reason)
                    .field("narrator_scene_mood", mood_key)
                    .field("intensity", mood_reasoning.classification.intensity)
                    .field("confidence", mood_reasoning.classification.confidence);
                    if !mood_reasoning.keyword_matches.is_empty() {
                        builder = builder.field(
                            "keyword_matches",
                            mood_reasoning
                                .keyword_matches
                                .iter()
                                .map(|(mood, kw)| format!("{}:{}", mood, kw))
                                .collect::<Vec<_>>(),
                        );
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
                        .send();
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
            }
            sidequest_game::MusicEvalResult::Suppressed { mood, intensity } => {
                // Same mood, low intensity — intentional suppression, not an error
                tracing::debug!(
                    mood = %mood,
                    intensity = intensity,
                    "music.suppressed — same mood, intensity below threshold"
                );
                WatcherEventBuilder::new("music_director", WatcherEventType::AgentSpanClose)
                    .field("turn_number", turn_approx)
                    .field(
                        "mood_classified",
                        mood_reasoning.classification.primary.as_str(),
                    )
                    .field("mood_reason", &mood_reasoning.reason)
                    .field("narrator_scene_mood", mood_key)
                    .field("suppressed", true)
                    .field("suppression_reason", "same_mood_low_intensity")
                    .field("suppressed_mood", &mood)
                    .field("suppressed_intensity", intensity)
                    .field("current_mood", &pre_telemetry.current_mood)
                    .field("current_track", &pre_telemetry.current_track)
                    .send();
            }
            sidequest_game::MusicEvalResult::NoTrackFound { mood, variation } => {
                // Genuine anomaly — mood/variation combo has no eligible tracks
                tracing::warn!(
                    mood = %mood,
                    variation = %variation,
                    "music.no_track_found — no eligible tracks for mood/variation"
                );
                WatcherEventBuilder::new("music_director", WatcherEventType::ValidationWarning)
                    .field("turn_number", turn_approx)
                    .field(
                        "mood_classified",
                        mood_reasoning.classification.primary.as_str(),
                    )
                    .field("mood_reason", &mood_reasoning.reason)
                    .field("narrator_scene_mood", mood_key)
                    .field("no_track_mood", &mood)
                    .field("no_track_variation", &variation)
                    .field("available_moods", &pre_telemetry.available_moods)
                    .field("tracks_per_mood", &pre_telemetry.tracks_per_mood)
                    .send();
            }
        }

        // Mood-driven image generation: when mood shifts, trigger a scene render.
        // Only fires when the classified mood differs from the previous mood,
        // preventing duplicate renders on stable moods.
        if pre_telemetry.current_mood.as_deref() != Some(mood_key) {
            if let Some(ref queue) = ctx.state.inner.render_queue {
                let mood_subject = sidequest_game::RenderSubject::new(
                    vec![],
                    sidequest_game::SceneType::Exploration,
                    sidequest_game::SubjectTier::Scene,
                    format!("{} atmosphere, {}", mood_key, ctx.current_location),
                    0.5,
                );
                if let Some(subject) = mood_subject {
                    // Rework Pass 1 finding #5: read visual_style.preferred_model,
                    // visual_style.lora, and visual_style.lora_trigger the same
                    // way dispatch/render.rs does, so mood images and scene
                    // images stay visually consistent within a session.
                    //
                    // Rework Pass 2 mirror: audio.rs must apply the same
                    // symmetric loud-failure and debounce patterns as
                    // dispatch/render.rs. The prior audio.rs code copied
                    // only the happy path — silent fall-through on
                    // (Some, None), silent `return None` on path traversal,
                    // no `lora_activated` telemetry, and no `tag_override`
                    // application. Fixes Findings B, C, D, E, and I.
                    //
                    // Finding I: apply location-based visual_tag_overrides
                    // so mood images in a "wasteland" location get the
                    // same genre-pack style overrides as scene images.
                    let location = extract_location_header(clean_narration)
                        .unwrap_or_default()
                        .to_lowercase();
                    let tag_override_opt = ctx.visual_style.as_ref().and_then(|vs| {
                        if location.is_empty() {
                            None
                        } else {
                            vs.visual_tag_overrides
                                .iter()
                                .find(|(key, _)| location.contains(key.as_str()))
                                .map(|(_, val)| val.clone())
                        }
                    });

                    let (art_style, model, neg_prompt, lora_path, lora_trigger, lora_scale) =
                        match ctx.visual_style {
                            Some(ref vs) => {
                                // Finding C+D+E: mirror render.rs
                                // (base_style, lora_active) pattern.
                                let (base_style, lora_active): (String, bool) = match (
                                    vs.lora.as_deref(),
                                    vs.lora_trigger.as_deref(),
                                ) {
                                    (Some(_), Some(trigger)) => (trigger.to_string(), true),
                                    (Some(lora), None) => {
                                        if ctx.state.mark_lora_warned(ctx.genre_slug) {
                                            tracing::warn!(
                                                lora = %lora,
                                                genre = %ctx.genre_slug,
                                                "lora set without lora_trigger — LoRA will load but trained style will not activate (silent no-op). Add lora_trigger to visual_style.yaml."
                                            );
                                            WatcherEventBuilder::new(
                                                "render",
                                                WatcherEventType::ValidationWarning,
                                            )
                                            .field("action", "lora_trigger_missing")
                                            .field("lora", lora)
                                            .field("genre", ctx.genre_slug)
                                            .send();
                                        }
                                        // Finding E: LoRA effectively disabled.
                                        (vs.positive_suffix.clone(), false)
                                    }
                                    _ => (vs.positive_suffix.clone(), false),
                                };

                                // Finding I: tag_override composition mirrors
                                // render.rs so scene+mood rendering stay
                                // visually consistent within a location.
                                let style = match tag_override_opt.as_deref() {
                                    Some(tag) => format!("{}, {}", tag, base_style),
                                    None => base_style,
                                };

                                // Finding B: loud failure on path traversal
                                // mirrors render.rs. Finding E: gate on
                                // lora_active so misconfigured LoRA does
                                // not fire lora_activated telemetry.
                                // Finding G: canonicalize both base and
                                // resolved to catch symlink escapes, and
                                // distinguish missing-file failures with a
                                // dedicated `lora_file_not_found` action
                                // code for GM-panel diagnosis.
                                let lora_abs: Option<String> = if lora_active {
                                    vs.lora.as_ref().and_then(|rel| {
                                        let base = ctx
                                            .state
                                            .genre_packs_path()
                                            .join(ctx.genre_slug);
                                        let resolved = base.join(rel);
                                        let base_canon = match std::fs::canonicalize(&base) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                tracing::error!(
                                                    base = %base.display(),
                                                    error = %e,
                                                    "lora base (genre pack dir) cannot be canonicalized — genre pack path is missing or inaccessible"
                                                );
                                                WatcherEventBuilder::new(
                                                    "render",
                                                    WatcherEventType::ValidationWarning,
                                                )
                                                .field("action", "lora_base_not_accessible")
                                                .field("base", base.to_string_lossy().as_ref())
                                                .field("genre", ctx.genre_slug)
                                                .send();
                                                return None;
                                            }
                                        };
                                        let resolved_canon = match std::fs::canonicalize(&resolved) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                tracing::error!(
                                                    lora = %rel,
                                                    genre = %ctx.genre_slug,
                                                    resolved = %resolved.display(),
                                                    error = %e,
                                                    "lora file not found or not accessible — genre pack references a LoRA file that cannot be canonicalized"
                                                );
                                                WatcherEventBuilder::new(
                                                    "render",
                                                    WatcherEventType::ValidationWarning,
                                                )
                                                .field("action", "lora_file_not_found")
                                                .field("lora", rel.as_str())
                                                .field("genre", ctx.genre_slug)
                                                .send();
                                                return None;
                                            }
                                        };
                                        if !resolved_canon.starts_with(&base_canon) {
                                            tracing::error!(
                                                lora = %rel,
                                                genre = %ctx.genre_slug,
                                                resolved = %resolved_canon.display(),
                                                base = %base_canon.display(),
                                                "lora path escapes genre pack directory (after canonicalization — catches symlink escapes) — rejecting."
                                            );
                                            WatcherEventBuilder::new(
                                                "render",
                                                WatcherEventType::ValidationWarning,
                                            )
                                            .field("action", "lora_path_traversal_rejected")
                                            .field("lora", rel.as_str())
                                            .field("genre", ctx.genre_slug)
                                            .send();
                                            return None;
                                        }
                                        Some(resolved_canon.to_string_lossy().into_owned())
                                    })
                                } else {
                                    None
                                };

                                // Mirror render.rs fallback: if LoRA was
                                // semantically active but the file didn't
                                // resolve, revert to positive_suffix so the
                                // daemon gets the real style description.
                                let style = if lora_active && lora_abs.is_none() {
                                    let fallback = match tag_override_opt.as_deref() {
                                        Some(tag) => format!("{}, {}", tag, vs.positive_suffix),
                                        None => vs.positive_suffix.clone(),
                                    };
                                    tracing::warn!(
                                        genre = %ctx.genre_slug,
                                        "lora file not resolved — reverting mood image art style from trigger word to positive_suffix"
                                    );
                                    fallback
                                } else {
                                    style
                                };

                                (
                                    style,
                                    vs.preferred_model.clone(),
                                    vs.negative_prompt.clone(),
                                    lora_abs,
                                    vs.lora_trigger.clone(),
                                    vs.lora_scale,
                                )
                            }
                            None => (
                                "oil_painting".to_string(),
                                String::new(),
                                String::new(),
                                None,
                                None,
                                None,
                            ),
                        };

                    // Mirror dispatch/render.rs: emit lora_activated watcher
                    // event BEFORE enqueue when the LoRA is active on mood
                    // image render. Without this emission the GM panel sees
                    // lora_activated for scene images but is silent on mood
                    // images — the exact compound opacity the reviewer's
                    // Rework Pass 2 Devil's Advocate section flagged.
                    if let Some(ref lora_abs) = lora_path {
                        WatcherEventBuilder::new(
                            "render",
                            WatcherEventType::SubsystemExerciseSummary,
                        )
                        .field("action", "lora_activated")
                        .field("lora_path", lora_abs.as_str())
                        .field("lora_trigger", lora_trigger.as_deref().unwrap_or(""))
                        .field("genre", ctx.genre_slug)
                        .field("source", "mood_image")
                        .send();
                    }

                    match queue
                        .enqueue(
                            subject.clone(),
                            &art_style,
                            &model,
                            &neg_prompt,
                            "",
                            lora_path.as_deref(),
                            lora_scale,
                        )
                        .await
                    {
                        Ok(sidequest_game::EnqueueResult::Queued { job_id }) => {
                            tracing::info!(%job_id, old_mood = ?pre_telemetry.current_mood, new_mood = %mood_key, "mood_image.queued — mood shift triggered scene render");
                            let dims = sidequest_game::tier_to_dimensions(subject.tier());
                            let _ = ctx
                                .tx
                                .send(sidequest_protocol::GameMessage::RenderQueued {
                                    payload: sidequest_protocol::RenderQueuedPayload {
                                        render_id: job_id.to_string(),
                                        tier: "scene".to_string(),
                                        width: dims.width,
                                        height: dims.height,
                                    },
                                    player_id: ctx.player_id.to_string(),
                                })
                                .await;
                            WatcherEventBuilder::new(
                                "mood_image",
                                WatcherEventType::StateTransition,
                            )
                            .field("action", "mood_image_queued")
                            .field(
                                "old_mood",
                                pre_telemetry.current_mood.as_deref().unwrap_or("none"),
                            )
                            .field("new_mood", mood_key)
                            .field("location", &*ctx.current_location)
                            .send();
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!(error = %e, "mood_image.enqueue_failed"),
                    }
                }
            }
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
                .send();
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
                .send();
            messages.push(GameMessage::AudioCue {
                payload: sidequest_protocol::AudioCuePayload {
                    mood: None,
                    music_track: None,
                    sfx_triggers: resolved_paths,
                    channel: Some("sfx".to_string()),
                    action: Some("play".to_string()),
                    volume: Some(0.7),
                    music_volume: None,
                    sfx_volume: None,
                    voice_volume: None,
                    crossfade_ms: None,
                },
                player_id: ctx.player_id.to_string(),
            });
        }
    }
}
