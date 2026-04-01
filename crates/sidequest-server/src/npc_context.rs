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
        let names: Vec<String> = registry.iter()
            .map(|e| if e.role.is_empty() {
                e.name.clone()
            } else {
                format!("{} ({})", e.name, e.role)
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

/// Build a name bank context string with pre-generated names for the narrator prompt.
///
/// Uses the Markov chain name generator to produce concrete names from each culture's
/// person_patterns and slot corpora. The narrator picks from these — no improvisation.
pub(crate) fn build_name_bank_context(
    cultures: &[sidequest_genre::Culture],
    corpus_dir: &std::path::Path,
) -> String {
    if cultures.is_empty() {
        return String::new();
    }

    let mut rng = rand::rng();
    let names_per_culture = 10;

    let mut lines = vec!["\n=== NPC NAME BANK (MANDATORY) ===\nYou MUST NOT invent NPC names. Pick from the pre-generated names below. If none fit, use a title or descriptor (\"the old mechanic\", \"the hooded stranger\"). Do NOT use generic Western fantasy names.".to_string()];

    for culture in cultures {
        let result = sidequest_genre::names::build_from_culture(culture, corpus_dir, &mut rng);
        let mut names: Vec<String> = Vec::with_capacity(names_per_culture);
        for _ in 0..names_per_culture {
            let name = result.generator.generate_person(&mut rng);
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }

        if names.is_empty() {
            continue;
        }

        lines.push(format!(
            "\n## {} — {}",
            culture.name.as_str(),
            culture.description
        ));
        for name in &names {
            lines.push(format!("  - {}", name));
        }
    }

    lines.join("\n")
}
