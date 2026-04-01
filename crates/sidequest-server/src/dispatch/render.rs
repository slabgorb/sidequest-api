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
    // Use narrator's visual_scene — the narrator already imagined the scene.
    let scene = match result.visual_scene {
        Some(ref vs) => vs,
        None => {
            tracing::error!("narrator did not provide visual_scene — skipping render");
            return;
        }
    };

    // Map narrator tier string to SubjectTier
    let tier = match scene.tier.as_str() {
        "portrait" => sidequest_game::SubjectTier::Portrait,
        "landscape" => sidequest_game::SubjectTier::Landscape,
        "scene_illustration" => sidequest_game::SubjectTier::Scene,
        _ => sidequest_game::SubjectTier::Scene,
    };

    // Build RenderSubject from narrator's visual description
    let subject = match sidequest_game::RenderSubject::new(
        vec![], // entities not needed — the subject text is already visual
        sidequest_game::SceneType::Exploration,
        tier,
        scene.subject.clone(),
        0.6, // default weight — narrator provided, always worth rendering
    ) {
        Some(s) => s,
        None => {
            tracing::error!(subject = %scene.subject, "invalid visual_scene from narrator");
            return;
        }
    };

    tracing::info!(
        prompt = %subject.prompt_fragment(),
        tier = ?subject.tier(),
        "visual_scene from narrator"
    );

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
            .send(ctx.state);
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
                .enqueue(subject, &art_style, &model, &neg_prompt, "")
                .await
            {
                Ok(r) => tracing::info!(result = ?r, "Render job enqueued"),
                Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
            }
        }
    }
}
