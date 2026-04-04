//! Server-side pre-generation for the Monster Manual (ADR-059).
//!
//! Calls tool binaries (namegen, encountergen) directly via `std::process::Command`
//! and populates the MonsterManual with results. Same invocation pattern as the
//! existing NPC gate in `update_npc_registry()`.

use std::process::{Command, Stdio};

use sidequest_game::monster_manual::MonsterManual;

use crate::AppState;

/// Call sidequest-namegen and return parsed JSON.
fn generate_npc(
    binary: &std::path::Path,
    genre_packs_path: &std::path::Path,
    genre: &str,
    culture: Option<&str>,
) -> Option<serde_json::Value> {
    let mut cmd = Command::new(binary);
    cmd.arg("--genre-packs-path")
        .arg(genre_packs_path)
        .arg("--genre")
        .arg(genre);
    if let Some(c) = culture {
        cmd.arg("--culture").arg(c);
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
pub fn seed_manual(state: &AppState, genre: &str, manual: &mut MonsterManual) {
    let span = tracing::info_span!(
        "pregen.seed_manual",
        genre = %genre,
        npcs_before = manual.npcs.len(),
        encounters_before = manual.encounters.len(),
    );
    let _guard = span.enter();

    let genre_packs_path = state.genre_packs_path();

    // ── NPCs: 1 per culture, up to 4 ──────────────────────
    if let Some(namegen_binary) = state.namegen_binary_path() {
        // Load genre pack to get culture names
        let genre_dir = genre_packs_path.join(genre);
        let cultures: Vec<String> = sidequest_genre::load_genre_pack(&genre_dir)
            .map(|pack| {
                pack.cultures
                    .iter()
                    .map(|c| c.name.as_str().to_string())
                    .take(4)
                    .collect()
            })
            .unwrap_or_default();

        const NPCS_PER_CULTURE: usize = 3;

        if cultures.is_empty() {
            // No cultures found — generate without culture flag
            for _ in 0..(NPCS_PER_CULTURE * 3) {
                if let Some(data) = generate_npc(namegen_binary, genre_packs_path, genre, None) {
                    tracing::info!(
                        name = data.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                        "pregen.npc_generated"
                    );
                    manual.add_npc(data, vec![]);
                }
            }
        } else {
            for culture in &cultures {
                for _ in 0..NPCS_PER_CULTURE {
                    if let Some(data) =
                        generate_npc(namegen_binary, genre_packs_path, genre, Some(culture))
                    {
                        tracing::info!(
                            name = data.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            culture = %culture,
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
            if let Some(data) =
                generate_encounter(encountergen_binary, genre_packs_path, genre, Some(tier), 2)
            {
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
