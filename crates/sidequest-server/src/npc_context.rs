//! NPC registry context builders for narrator prompt injection.

use sidequest_game::NpcRegistryEntry;

/// Build budgeted NPC registry context.
///
/// If `references_npc` is true (player mentioned an NPC), scene-present NPCs
/// get full entries with appearance, personality, and identity-lock rules.
/// If false, all NPCs get compact name+role only — the narrator doesn't need
/// full profiles when the player isn't interacting with anyone.
pub(crate) fn build_npc_registry_context_budgeted(
    registry: &[NpcRegistryEntry],
    current_turn: u32,
    references_npc: bool,
) -> String {
    if registry.is_empty() {
        return String::new();
    }

    if !references_npc {
        // Compact: just names so the narrator doesn't invent duplicates
        let names: Vec<String> = registry
            .iter()
            .map(|e| {
                if e.role.is_empty() {
                    e.name.clone()
                } else {
                    format!("{} ({})", e.name, e.role)
                }
            })
            .collect();
        return format!("\nKnown NPCs: {}", names.join(", "));
    }

    // Full profiles for scene-present NPCs, name+role for others
    let mut scene_npcs = Vec::new();
    let mut background_names = Vec::new();

    for entry in registry {
        if current_turn.saturating_sub(entry.last_seen_turn) <= 2 {
            scene_npcs.push(entry);
        } else {
            let label = if entry.role.is_empty() {
                entry.name.clone()
            } else {
                format!("{} ({})", entry.name, entry.role)
            };
            background_names.push(label);
        }
    }

    let mut lines = Vec::new();

    if !scene_npcs.is_empty() {
        lines.push("\n=== SCENE NPCs (NOT the player) — CANONICAL IDENTITY (do NOT contradict, do NOT apply player abilities/backstory to these NPCs) ===".to_string());
        for entry in &scene_npcs {
            let mut desc = format!("- {}", entry.name);
            if !entry.pronouns.is_empty() {
                desc.push_str(&format!(" ({})", entry.pronouns));
            }
            if !entry.role.is_empty() {
                desc.push_str(&format!(", {}", entry.role));
            }
            let mut physical: Vec<&str> = Vec::new();
            if !entry.age.is_empty() {
                physical.push(&entry.age);
            }
            if !entry.appearance.is_empty() {
                physical.push(&entry.appearance);
            }
            if !physical.is_empty() {
                desc.push_str(&format!(" [{}]", physical.join("; ")));
            }
            if !entry.ocean_summary.is_empty() {
                desc.push_str(&format!(" | personality: {}", entry.ocean_summary));
            }
            lines.push(desc);
        }
    }

    if !background_names.is_empty() {
        lines.push(format!("\nAlso known: {}", background_names.join(", ")));
    }

    lines.join("\n")
}

/// Build a slim culture reference for the narrator prompt.
///
/// Lists available culture names and descriptions so the narrator knows what
/// `--culture` values to pass to `sidequest-namegen`. No pre-generated names —
/// the narrator calls the tool at runtime.
pub(crate) fn build_culture_reference(cultures: &[sidequest_genre::Culture]) -> String {
    if cultures.is_empty() {
        return String::new();
    }

    let mut lines = vec!["\n=== AVAILABLE CULTURES ===".to_string()];
    for culture in cultures {
        lines.push(format!(
            "- {} — {}",
            culture.name.as_str(),
            culture.description
        ));
    }
    lines.join("\n")
}
