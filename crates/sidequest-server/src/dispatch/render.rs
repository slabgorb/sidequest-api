//! Image render pipeline — visual scene extraction and render queue.

use crate::extraction::extract_location_header;

use super::DispatchContext;

/// Render pipeline — use narrator's visual_scene for image prompts.
#[tracing::instrument(name = "turn.render", skip_all)]
pub(crate) async fn process_render(
    ctx: &mut DispatchContext<'_>,
    _clean_narration: &str,
    narration_text: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
) {
    // Render subject: prefer narrator's visual_scene, fall back to SubjectExtractor
    // parsing the narration text.  Without this fallback, ensemble/dialogue turns
    // (which don't produce visual_scene) generate zero images — a wiring gap where
    // SubjectExtractor existed and worked but was never connected.
    let subject = if let Some(ref scene) = result.visual_scene {
        let tier = match scene.tier.as_str() {
            "portrait" => sidequest_game::SubjectTier::Portrait,
            "landscape" => sidequest_game::SubjectTier::Landscape,
            "scene_illustration" => sidequest_game::SubjectTier::Scene,
            _ => sidequest_game::SubjectTier::Scene,
        };
        match sidequest_game::RenderSubject::new(
            vec![],
            sidequest_game::SceneType::Exploration,
            tier,
            scene.subject.clone(),
            0.6,
        ) {
            Some(s) => {
                tracing::info!(
                    prompt = %s.prompt_fragment(),
                    tier = ?s.tier(),
                    "render.visual_scene_from_narrator"
                );
                s
            }
            None => {
                tracing::error!(subject = %scene.subject, "invalid visual_scene from narrator");
                return;
            }
        }
    } else {
        // SubjectExtractor fallback — parse narration text for render subjects.
        let extraction_ctx = sidequest_game::ExtractionContext {
            in_combat: ctx.combat_state.in_combat(),
            known_npcs: ctx.npc_registry.iter().map(|e| e.name.clone()).collect(),
            current_location: ctx.current_location.clone(),
            recent_subjects: vec![],
        };
        let extractor = sidequest_game::SubjectExtractor::new();
        match extractor.extract(_clean_narration, &extraction_ctx) {
            Some(s) => {
                tracing::info!(
                    prompt = %s.prompt_fragment(),
                    tier = ?s.tier(),
                    "render.subject_extracted_from_narration"
                );
                s
            }
            None => {
                tracing::debug!("render.no_subject_extracted — narration too short or low-weight");
                return;
            }
        }
    };

    // Scene relevance validation — reject prompts that don't match the current scene
    let relevance_ctx = sidequest_game::ExtractionContext {
        in_combat: ctx.combat_state.in_combat(),
        known_npcs: ctx.npc_registry.iter().map(|e| e.name.clone()).collect(),
        current_location: ctx.current_location.clone(),
        recent_subjects: vec![],
    };
    let validator = sidequest_game::SceneRelevanceValidator::new();
    let verdict = validator.evaluate(&subject, &relevance_ctx);
    if verdict.is_rejected() {
        tracing::warn!(
            reason = verdict.reason(),
            prompt = %subject.prompt_fragment(),
            "scene_relevance.rejected — skipping render"
        );
        crate::WatcherEventBuilder::new("render", crate::WatcherEventType::ValidationWarning)
            .severity(crate::Severity::Warn)
            .field("action", "scene_relevance_rejected")
            .field("reason", verdict.reason())
            .field("prompt", subject.prompt_fragment())
            .send();
        return;
    }

    let filter_ctx = sidequest_game::FilterContext {
        in_combat: ctx.combat_state.in_combat(),
        scene_transition: extract_location_header(narration_text).is_some(),
        player_requested: false,
    };
    let decision = ctx
        .state
        .inner
        .beat_filter
        .lock()
        .await
        .evaluate(&subject, &filter_ctx);
    tracing::info!(decision = ?decision, "BeatFilter decision");
    if matches!(decision, sidequest_game::FilterDecision::Render { .. }) {
        if let Some(ref queue) = ctx.state.inner.render_queue {
            let (art_style, model, neg_prompt) = match ctx.visual_style {
                Some(ref vs) => {
                    let location = extract_location_header(narration_text)
                        .unwrap_or_default()
                        .to_lowercase();
                    let tag_override = if !location.is_empty() {
                        vs.visual_tag_overrides
                            .iter()
                            .find(|(key, _)| location.contains(key.as_str()))
                            .map(|(_, val)| val.as_str())
                    } else {
                        None
                    };
                    let style = match tag_override {
                        Some(tag) => format!("{}, {}", tag, vs.positive_suffix),
                        None => vs.positive_suffix.clone(),
                    };
                    (
                        style,
                        vs.preferred_model.clone(),
                        vs.negative_prompt.clone(),
                    )
                }
                None => (
                    "oil_painting".to_string(),
                    "flux-schnell".to_string(),
                    String::new(),
                ),
            };
            // Send visual_scene subject as prompt — no narration, daemon skips SubjectExtractor
            match queue
                .enqueue(subject.clone(), &art_style, &model, &neg_prompt, "")
                .await
            {
                Ok(sidequest_game::EnqueueResult::Queued { job_id }) => {
                    tracing::info!(%job_id, "Render job enqueued");
                    // Notify UI to show placeholder shimmer while Flux generates
                    let dims = sidequest_game::tier_to_dimensions(subject.tier());
                    let _ = ctx.tx.send(sidequest_protocol::GameMessage::RenderQueued {
                        payload: sidequest_protocol::RenderQueuedPayload {
                            render_id: job_id.to_string(),
                            tier: format!("{:?}", subject.tier()).to_lowercase(),
                            width: dims.width,
                            height: dims.height,
                        },
                        player_id: ctx.player_id.to_string(),
                    }).await;
                }
                Ok(r) => tracing::info!(result = ?r, "Render job deduplicated"),
                Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
            }
        }
    }
}
