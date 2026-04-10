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
            in_combat: ctx.in_combat(),
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
        in_combat: ctx.in_combat(),
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
        in_combat: ctx.in_combat(),
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
            // Story 35-15: LoRA wiring. Per ADR-032, when the genre pack's
            // visual_style declares a `lora`, resolve it to an absolute path
            // (relative to the genre pack dir), substitute the `lora_trigger`
            // for `positive_suffix` in the composed CLIP prompt, and emit a
            // `render / lora_activated` watcher event so the GM panel can
            // verify the LoRA is engaged (the daemon-side span attributes
            // do NOT surface to the watcher WebSocket — the Rust emission
            // is authoritative).
            //
            // The daemon does NOT auto-prepend trigger words (verified
            // against flux_mlx_worker.py:206 `_compose_prompt()`); the
            // substitution happens here in Rust.
            let (art_style, model, neg_prompt, lora_path, lora_trigger, lora_scale) = match ctx
                .visual_style
            {
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

                    // When a LoRA is active AND a trigger word is provided,
                    // substitute the trigger for positive_suffix per
                    // ADR-032 ("The positive_suffix is dropped from the
                    // CLIP prompt entirely when a LoRA is active").
                    //
                    // Rework finding #1: the (Some, None) case — LoRA set
                    // without a trigger — is a silent no-op. The LoRA
                    // loads but the trained style never activates. Emit a
                    // tracing::warn! AND a WatcherEventBuilder
                    // ValidationWarning so the GM panel surfaces the
                    // misconfiguration.
                    let base_style: String = match (vs.lora.as_deref(), vs.lora_trigger.as_deref())
                    {
                        (Some(_), Some(trigger)) => trigger.to_string(),
                        (Some(lora), None) => {
                            tracing::warn!(
                                lora = %lora,
                                genre = %ctx.genre_slug,
                                "lora set without lora_trigger — LoRA will load but trained style will not activate (silent no-op). Add lora_trigger to visual_style.yaml."
                            );
                            crate::WatcherEventBuilder::new(
                                "render",
                                crate::WatcherEventType::ValidationWarning,
                            )
                            .field("action", "lora_trigger_missing")
                            .field("lora", lora)
                            .field("genre", ctx.genre_slug)
                            .send();
                            vs.positive_suffix.clone()
                        }
                        _ => vs.positive_suffix.clone(),
                    };
                    let style = match tag_override {
                        Some(tag) => format!("{}, {}", tag, base_style),
                        None => base_style,
                    };

                    // Resolve LoRA path relative to the genre pack dir.
                    // Rework finding #6: validate the resolved path stays
                    // inside the genre pack directory. PathBuf::join does
                    // not sanitize, so a YAML `lora: ../../../etc/passwd`
                    // would escape. Check with starts_with() against the
                    // expected genre pack base dir. Fail loudly if escape.
                    let lora_abs: Option<String> = vs.lora.as_ref().and_then(|rel| {
                        let base = ctx.state.genre_packs_path().join(ctx.genre_slug);
                        let resolved = base.join(rel);
                        if !resolved.starts_with(&base) {
                            tracing::error!(
                                lora = %rel,
                                genre = %ctx.genre_slug,
                                resolved = %resolved.display(),
                                "lora path escapes genre pack directory — rejecting. Relative paths with `..` segments are not allowed."
                            );
                            crate::WatcherEventBuilder::new("render", crate::WatcherEventType::ValidationWarning)
                                .field("action", "lora_path_traversal_rejected")
                                .field("lora", rel.as_str())
                                .field("genre", ctx.genre_slug)
                                .send();
                            return None;
                        }
                        Some(resolved.to_string_lossy().into_owned())
                    });

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
                    // Empty variant → daemon falls back to per-tier default in
                    // flux_mlx_worker.py TIER_CONFIGS. The pre-existing dead
                    // variant literal was removed because it was never a
                    // valid daemon variant ("dev"/"schnell" are the only
                    // accepted values). Pre-existing silent fallback for
                    // missing visual_style is still a Delivery Finding — a
                    // dedicated story should close the whole None branch.
                    String::new(),
                    String::new(),
                    None,
                    None,
                    None,
                ),
            };

            // Emit watcher event BEFORE enqueue when LoRA is active — the
            // GM panel's lie-detector signal per CLAUDE.md OTEL principle.
            if let Some(ref lora_abs) = lora_path {
                crate::WatcherEventBuilder::new(
                    "render",
                    crate::WatcherEventType::SubsystemExerciseSummary,
                )
                .field("action", "lora_activated")
                .field("lora_path", lora_abs.as_str())
                .field("lora_trigger", lora_trigger.as_deref().unwrap_or(""))
                .field("genre", ctx.genre_slug)
                .send();
            }

            // Send visual_scene subject as prompt — no narration, daemon skips SubjectExtractor.
            // `model` carries `visual_style.preferred_model` as the Flux variant override
            // (empty string → daemon picks per-tier). Story 35-15 closed the dead wire
            // where this was previously silently dropped at the `_image_model` parameter.
            match queue
                .enqueue(
                    subject.clone(),
                    &art_style,
                    &model,
                    &neg_prompt,
                    "",
                    lora_path.as_deref(),
                    lora_scale, // from visual_style.lora_scale; None lets daemon default to 1.0
                )
                .await
            {
                Ok(sidequest_game::EnqueueResult::Queued { job_id }) => {
                    tracing::info!(%job_id, "Render job enqueued");
                    // Notify UI to show placeholder shimmer while Flux generates
                    let dims = sidequest_game::tier_to_dimensions(subject.tier());
                    let _ = ctx
                        .tx
                        .send(sidequest_protocol::GameMessage::RenderQueued {
                            payload: sidequest_protocol::RenderQueuedPayload {
                                render_id: job_id.to_string(),
                                tier: format!("{:?}", subject.tier()).to_lowercase(),
                                width: dims.width,
                                height: dims.height,
                            },
                            player_id: ctx.player_id.to_string(),
                        })
                        .await;
                }
                Ok(r) => tracing::info!(result = ?r, "Render job deduplicated"),
                Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
            }
        }
    }
}
