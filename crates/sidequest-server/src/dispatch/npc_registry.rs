//! NPC registry updates from structured narrator output + OCEAN personality shifts.

use sidequest_genre::{GenreCode, GenreLoader};

use crate::{NpcRegistryEntry, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Update NPC registry from structured narrator output and apply OCEAN personality shifts.
/// Returns a list of (npc_name, served_url) for any pre-rendered images found
/// for newly registered NPCs. Checks both `images/creatures/` (bestiary) and
/// `images/portraits/` (NPC portraits) in the genre pack. The caller broadcasts
/// these as `GameMessage::Image` (async send via `ctx.tx`).
pub(super) fn update_npc_registry(
    ctx: &mut DispatchContext<'_>,
    result: &sidequest_agents::orchestrator::ActionResult,
    _clean_narration: &str,
) -> Vec<(String, String)> {
    let mut creature_images: Vec<(String, String)> = Vec::new();
    let turn_approx = ctx.turn_manager.interaction() as u32;
    if !result.npcs_present.is_empty() {
        let mut npcs_sorted: Vec<_> = result.npcs_present.iter().collect();
        npcs_sorted.sort_by(|a, b| b.name.len().cmp(&a.name.len()));
        tracing::info!(
            count = npcs_sorted.len(),
            "npc_registry.structured — updating from narrator JSON"
        );
        for npc in &npcs_sorted {
            if npc.name.is_empty() {
                continue;
            }
            let name_lower = npc.name.to_lowercase();
            if let Some(entry) = ctx.npc_registry.iter_mut().find(|e| {
                e.name.to_lowercase() == name_lower
                    || e.name.to_lowercase().contains(&name_lower)
                    || name_lower.contains(&e.name.to_lowercase())
            }) {
                entry.last_seen_turn = turn_approx;
                if !ctx.current_location.is_empty() {
                    entry.location = ctx.current_location.to_string();
                }
                if npc.name.len() > entry.name.len() {
                    entry.name = npc.name.clone();
                }
            } else {
                let span = tracing::info_span!(
                    "npc.registration",
                    npc_name = %npc.name,
                    npc_role = %npc.role,
                    ocean_summary = tracing::field::Empty,
                    archetype_source = tracing::field::Empty,
                    namegen_validated = tracing::field::Empty,
                    genre = %ctx.genre_slug,
                );
                let _guard = span.enter();

                let namegen_result = ctx.state.namegen_binary_path().and_then(|binary| {
                    let output = std::process::Command::new(binary)
                        .arg("--genre-packs-path")
                        .arg(ctx.state.genre_packs_path())
                        .arg("--genre")
                        .arg(ctx.genre_slug)
                        .arg("--role")
                        .arg(if npc.role.is_empty() {
                            "unknown"
                        } else {
                            &npc.role
                        })
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output()
                        .ok()?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        tracing::warn!(
                            error = %stderr,
                            "npc_gate.namegen_failed — falling back to narrator-provided identity"
                        );
                        return None;
                    }
                    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
                });

                let (ocean_profile, ocean_summary, source) = if let Some(ref gen) = namegen_result {
                    let profile = gen
                        .get("ocean")
                        .and_then(|o| {
                            Some(sidequest_genre::OceanProfile {
                                openness: o.get("openness")?.as_f64()?,
                                conscientiousness: o.get("conscientiousness")?.as_f64()?,
                                extraversion: o.get("extraversion")?.as_f64()?,
                                agreeableness: o.get("agreeableness")?.as_f64()?,
                                neuroticism: o.get("neuroticism")?.as_f64()?,
                            })
                        })
                        .unwrap_or_else(sidequest_genre::OceanProfile::random);
                    let summary = gen
                        .get("ocean_summary")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| profile.behavioral_summary());
                    let src = gen
                        .get("archetype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("namegen")
                        .to_string();
                    (profile, summary, src)
                } else {
                    let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
                    let from_pack = GenreCode::new(ctx.genre_slug)
                        .ok()
                        .and_then(|code| loader.load(&code).ok())
                        .and_then(|pack| {
                            let with_ocean: Vec<_> = pack
                                .archetypes
                                .iter()
                                .filter(|a| a.ocean.is_some())
                                .collect();
                            if with_ocean.is_empty() {
                                return None;
                            }
                            use rand::prelude::IndexedRandom;
                            let arch = with_ocean.choose(&mut rand::rng())?;
                            let profile = arch.ocean.as_ref()?.with_jitter(1.5);
                            Some((profile, arch.name.as_str().to_string()))
                        })
                        .unwrap_or_else(|| {
                            (
                                sidequest_genre::OceanProfile::random(),
                                "random".to_string(),
                            )
                        });
                    let summary = from_pack.0.behavioral_summary();
                    (from_pack.0, summary, from_pack.1)
                };

                let validated = namegen_result.is_some();
                span.record("namegen_validated", validated);
                span.record("ocean_summary", &ocean_summary.as_str());
                span.record("archetype_source", &source.as_str());

                if !validated && ctx.state.namegen_binary_path().is_some() {
                    tracing::warn!(
                        npc_name = %npc.name,
                        "npc_gate.validation_warning — namegen binary available but generation failed; narrator name accepted without tool verification"
                    );
                }

                tracing::info!(
                    name = %npc.name, pronouns = %npc.pronouns, role = %npc.role,
                    ocean = %ocean_summary, archetype = %source,
                    namegen_validated = validated,
                    "npc_registry.new — registered with {} identity",
                    if validated { "namegen-enriched" } else { "fallback" }
                );

                let npc_slug = npc
                    .name
                    .to_lowercase()
                    .replace(' ', "_")
                    .replace('\'', "")
                    .replace('\u{2019}', "");
                let mut npc_image_url: Option<String> = None;

                let creature_image_path = ctx
                    .state
                    .genre_packs_path()
                    .join(ctx.genre_slug)
                    .join("images")
                    .join("creatures")
                    .join(format!("{}.png", npc_slug));
                if creature_image_path.exists() {
                    let served_url = format!(
                        "/genre/{}/images/creatures/{}.png",
                        ctx.genre_slug, npc_slug
                    );
                    tracing::info!(
                        creature = %npc.name,
                        url = %served_url,
                        "creature_image.served — pre-rendered bestiary image on first appearance"
                    );
                    creature_images.push((npc.name.clone(), served_url.clone()));
                    npc_image_url = Some(served_url);
                    WatcherEventBuilder::new("creature_image", WatcherEventType::StateTransition)
                        .field("action", "creature_image_served")
                        .field("creature", &npc.name)
                        .field("slug", &npc_slug)
                        .send();
                } else {
                    let portrait_image_path = ctx
                        .state
                        .genre_packs_path()
                        .join(ctx.genre_slug)
                        .join("images")
                        .join("portraits")
                        .join(format!("{}.png", npc_slug));
                    if portrait_image_path.exists() {
                        let served_url = format!(
                            "/genre/{}/images/portraits/{}.png",
                            ctx.genre_slug, npc_slug
                        );
                        tracing::info!(
                            npc = %npc.name,
                            url = %served_url,
                            "npc_portrait.served — pre-rendered portrait on first appearance"
                        );
                        creature_images.push((npc.name.clone(), served_url.clone()));
                        npc_image_url = Some(served_url);
                        WatcherEventBuilder::new("npc_portrait", WatcherEventType::StateTransition)
                            .field("action", "npc_portrait_served")
                            .field("npc", &npc.name)
                            .field("slug", &npc_slug)
                            .send();
                    }
                }

                ctx.npc_registry.push(NpcRegistryEntry {
                    name: npc.name.clone(),
                    pronouns: npc.pronouns.clone(),
                    role: npc.role.clone(),
                    age: String::new(),
                    appearance: npc.appearance.clone(),
                    location: ctx.current_location.to_string(),
                    last_seen_turn: turn_approx,
                    ocean_summary: ocean_summary.clone(),
                    ocean: Some(ocean_profile),
                    hp: 0,
                    max_hp: 0,
                    portrait_url: npc_image_url,
                });
                WatcherEventBuilder::new("npc_registry", WatcherEventType::StateTransition)
                    .field("action", "npc_registered")
                    .field("name", &npc.name)
                    .field("role", &npc.role)
                    .field("ocean", &ocean_summary)
                    .field("namegen_validated", validated)
                    .field("archetype_source", &source)
                    .field("registry_size", ctx.npc_registry.len())
                    .send();
            }
        }
    }
    WatcherEventBuilder::new("npc_registry", WatcherEventType::SubsystemExerciseSummary)
        .field("event", "npc_registry.scan")
        .field("npcs_in_narration", result.npcs_present.len())
        .field("registry_size", ctx.npc_registry.len())
        .field("turn", ctx.turn_manager.interaction())
        .send();

    // OCEAN personality shifts
    {
        let personality_events: Vec<(String, sidequest_game::PersonalityEvent)> = result
            .personality_events
            .iter()
            .map(|pe| (pe.npc.clone(), pe.event_type))
            .collect();

        if !personality_events.is_empty() {
            let (applied, shift_log) = sidequest_game::apply_ocean_shifts(
                ctx.npc_registry,
                &personality_events,
                turn_approx,
            );
            if !applied.is_empty() {
                WatcherEventBuilder::new("ocean", WatcherEventType::StateTransition)
                    .field("event", "ocean.shift_applied")
                    .field("shifts_applied", applied.len())
                    .field("personality_events", personality_events.len())
                    .field("shift_log_entries", shift_log.shifts().len())
                    .field("turn", turn_approx)
                    .send();

                for proposal in &applied {
                    WatcherEventBuilder::new("ocean", WatcherEventType::StateTransition)
                        .field("event", "ocean.shift_proposed")
                        .field("npc_name", &proposal.npc_name)
                        .field("dimension", format!("{:?}", proposal.dimension))
                        .field("delta", format!("{:.2}", proposal.delta))
                        .field("cause", &proposal.cause)
                        .field("turn", turn_approx)
                        .send();
                }
            }
        }
    }

    creature_images
}
