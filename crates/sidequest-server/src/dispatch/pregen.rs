//! Server-side pre-generation for the Monster Manual (ADR-059).
//!
//! Calls tool binaries (namegen, encountergen) directly via `std::process::Command`
//! and populates the MonsterManual with results. Same invocation pattern as the
//! existing NPC gate in `update_npc_registry()`.

use std::process::{Command, Stdio};

use rand::Rng;
use sidequest_game::monster_manual::MonsterManual;
use sidequest_genre::models::archetype_constraints::ArchetypeConstraints;

use crate::AppState;

/// Select diverse [jungian, rpg_role, npc_role] combinations for pre-gen pool.
/// Weighted toward common pairings but includes uncommon/rare for diversity.
fn select_diverse_pairings(
    constraints: &ArchetypeConstraints,
    count: usize,
    rng: &mut impl Rng,
) -> Vec<(String, String, String)> {
    let common_count = (count as f64 * 0.6).ceil() as usize;
    let uncommon_count = (count as f64 * 0.3).ceil() as usize;
    let rare_count = count.saturating_sub(common_count + uncommon_count);

    let sample = |list: &[[String; 2]], n: usize, rng: &mut dyn rand::RngCore| -> Vec<(String, String)> {
        if list.is_empty() {
            return vec![];
        }
        (0..n)
            .map(|_| {
                let pair = &list[rng.next_u64() as usize % list.len()];
                (pair[0].clone(), pair[1].clone())
            })
            .collect()
    };

    let mut pool = Vec::with_capacity(count);
    pool.extend(sample(&constraints.valid_pairings.common, common_count, rng));
    pool.extend(sample(&constraints.valid_pairings.uncommon, uncommon_count, rng));
    pool.extend(sample(&constraints.valid_pairings.rare, rare_count, rng));

    let npc_roles = &constraints.npc_roles_available;
    if npc_roles.is_empty() {
        return pool
            .into_iter()
            .map(|(j, r)| (j, r, String::new()))
            .collect();
    }
    pool.into_iter()
        .enumerate()
        .map(|(i, (j, r))| {
            let role = &npc_roles[i % npc_roles.len()];
            (j, r, role.clone())
        })
        .collect()
}

/// Call sidequest-namegen and return parsed JSON.
fn generate_npc(
    binary: &std::path::Path,
    genre_packs_path: &std::path::Path,
    genre: &str,
    culture: Option<&str>,
    axes: Option<(&str, &str, &str)>,
    world: Option<&str>,
) -> Option<serde_json::Value> {
    let mut cmd = Command::new(binary);
    cmd.arg("--genre-packs-path")
        .arg(genre_packs_path)
        .arg("--genre")
        .arg(genre);
    if let Some(c) = culture {
        cmd.arg("--culture").arg(c);
    }
    if let Some((jungian, rpg_role, npc_role)) = axes {
        if !jungian.is_empty() {
            cmd.arg("--jungian").arg(jungian);
        }
        if !rpg_role.is_empty() {
            cmd.arg("--rpg-role").arg(rpg_role);
        }
        if !npc_role.is_empty() {
            cmd.arg("--npc-role").arg(npc_role);
        }
    }
    if let Some(w) = world {
        if !w.is_empty() {
            cmd.arg("--world").arg(w);
        }
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(error = %stderr, "pregen.namegen_failed");
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
}

/// Call sidequest-encountergen and return parsed JSON.
fn generate_encounter(
    binary: &std::path::Path,
    genre_packs_path: &std::path::Path,
    genre: &str,
    world: &str,
    tier: Option<u32>,
    count: u32,
) -> Option<serde_json::Value> {
    let mut cmd = Command::new(binary);
    cmd.arg("--genre-packs-path")
        .arg(genre_packs_path)
        .arg("--genre")
        .arg(genre)
        .arg("--count")
        .arg(count.to_string());
    if !world.is_empty() {
        cmd.arg("--world").arg(world);
    }
    if let Some(t) = tier {
        cmd.arg("--tier").arg(t.to_string());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(error = %stderr, "pregen.encountergen_failed");
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
}

/// Seed a MonsterManual with NPCs and encounters from tool binaries.
///
/// Examines the genre pack's cultures and generates 3 NPCs per culture (up to 12 total).
/// Generates 2 encounter blocks (tier 1 and tier 2).
/// When `world` is provided, encountergen reads `worlds/{world}/creatures.yaml` for
/// creature definitions instead of generating humanoid NPCs from rules.yaml.
pub fn seed_manual(state: &AppState, genre: &str, world: &str, manual: &mut MonsterManual) {
    let span = tracing::info_span!(
        "pregen.seed_manual",
        genre = %genre,
        npcs_before = manual.npcs.len(),
        encounters_before = manual.encounters.len(),
    );
    let _guard = span.enter();

    let genre_packs_path = state.genre_packs_path();

    // ── NPCs: 3 per culture, up to 4 cultures ─────────────
    if let Some(namegen_binary) = state.namegen_binary_path() {
        // Load genre pack to get cultures and archetype constraints
        let genre_dir = genre_packs_path.join(genre);
        let pack = sidequest_genre::load_genre_pack(&genre_dir);

        let (cultures, constraints) = match &pack {
            Ok(p) => {
                let cultures: Vec<String> = p
                    .cultures
                    .iter()
                    .map(|c| c.name.as_str().to_string())
                    .take(4)
                    .collect();
                (cultures, p.archetype_constraints.as_ref())
            }
            Err(_) => (vec![], None),
        };

        const NPCS_PER_CULTURE: usize = 3;
        let mut rng = rand::rng();

        // Pre-compute axis pairings when constraints are available
        let npc_count = if cultures.is_empty() {
            NPCS_PER_CULTURE * 3
        } else {
            cultures.len() * NPCS_PER_CULTURE
        };
        let pairings: Option<Vec<(String, String, String)>> = constraints
            .map(|c| select_diverse_pairings(c, npc_count, &mut rng));

        let world_opt = if world.is_empty() { None } else { Some(world) };

        if cultures.is_empty() {
            // No cultures found — generate without culture flag
            for i in 0..npc_count {
                let axes = pairings.as_ref().and_then(|p| p.get(i)).map(|(j, r, n)| {
                    (j.as_str(), r.as_str(), n.as_str())
                });
                if let Some(data) = generate_npc(
                    namegen_binary,
                    genre_packs_path,
                    genre,
                    None,
                    axes,
                    world_opt,
                ) {
                    tracing::info!(
                        name = data.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        jungian = axes.map(|(j, _, _)| j).unwrap_or(""),
                        rpg_role = axes.map(|(_, r, _)| r).unwrap_or(""),
                        npc_role = axes.map(|(_, _, n)| n).unwrap_or(""),
                        "pregen.npc_generated"
                    );
                    manual.add_npc(data, vec![]);
                }
            }
        } else {
            for (ci, culture) in cultures.iter().enumerate() {
                for j in 0..NPCS_PER_CULTURE {
                    let pairing_idx = ci * NPCS_PER_CULTURE + j;
                    let axes = pairings
                        .as_ref()
                        .and_then(|p| p.get(pairing_idx))
                        .map(|(jungian, rpg_role, npc_role)| {
                            (jungian.as_str(), rpg_role.as_str(), npc_role.as_str())
                        });
                    if let Some(data) = generate_npc(
                        namegen_binary,
                        genre_packs_path,
                        genre,
                        Some(culture),
                        axes,
                        world_opt,
                    ) {
                        tracing::info!(
                            name = data.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            culture = %culture,
                            jungian = axes.map(|(jungian, _, _)| jungian).unwrap_or(""),
                            rpg_role = axes.map(|(_, rpg_role, _)| rpg_role).unwrap_or(""),
                            npc_role = axes.map(|(_, _, npc_role)| npc_role).unwrap_or(""),
                            "pregen.npc_generated"
                        );
                        manual.add_npc(data, vec![]);
                    }
                }
            }
        }
    } else {
        tracing::warn!("pregen.namegen_binary_missing — cannot seed NPCs");
    }

    // ── Encounters: tier 1 + tier 2 ───────────────────────
    if let Some(encountergen_binary) = state.encountergen_binary_path() {
        for tier in [1u32, 2] {
            if let Some(data) = generate_encounter(
                encountergen_binary,
                genre_packs_path,
                genre,
                world,
                Some(tier),
                2,
            ) {
                tracing::info!(tier = tier, "pregen.encounter_generated");
                manual.add_encounter(data, tier, vec![]);
            }
        }
    } else {
        tracing::warn!("pregen.encountergen_binary_missing — cannot seed encounters");
    }

    tracing::info!(
        npcs_after = manual.npcs.len(),
        encounters_after = manual.encounters.len(),
        "pregen.seed_manual_complete"
    );

    manual.save();
}
