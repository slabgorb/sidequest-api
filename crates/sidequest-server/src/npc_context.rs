//! NPC registry context builders for narrator prompt injection.

use sidequest_game::NpcRegistryEntry;

/// Build the NPC registry context string for the narrator prompt.
pub(crate) fn build_npc_registry_context(registry: &[NpcRegistryEntry]) -> String {
    if registry.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\nACTIVE NPCs — CANONICAL IDENTITY (do NOT contradict):\nThese NPCs have been established in this session. Their names, pronouns, gender, physical appearance, and roles are LOCKED. If an NPC was described as male (\"Big man, missing an ear\"), they stay male in ALL future narration. Never flip gender, change names, or alter physical descriptions:".to_string()];
    for entry in registry {
        let mut desc = format!("- {}", entry.name);
        if !entry.pronouns.is_empty() {
            desc.push_str(&format!(" ({})", entry.pronouns));
        }
        if !entry.role.is_empty() {
            desc.push_str(&format!(", {}", entry.role));
        }
        // Physical description — age and appearance are identity-locked
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
        if !entry.location.is_empty() {
            desc.push_str(&format!(" — at {}", entry.location));
        }
        lines.push(desc);
    }
    lines.join("\n")
}

/// Build budgeted NPC registry context — scene-present NPCs get full entries,
/// others get name+role only.
pub(crate) fn build_npc_registry_context_budgeted(
    registry: &[NpcRegistryEntry],
    current_turn: u32,
) -> String {
    if registry.is_empty() {
        return String::new();
    }

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
        lines.push("\nSCENE NPCs — CANONICAL IDENTITY (do NOT contradict):".to_string());
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

/// Build a name bank context string from genre pack cultures for the narrator prompt.
/// Extracts word lists and person name patterns so the LLM uses culturally appropriate names.
pub(crate) fn build_name_bank_context(cultures: &[sidequest_genre::Culture]) -> String {
    if cultures.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\nNAME BANKS — When introducing new NPCs, you MUST draw names from these cultural name banks. Do NOT use generic Western fantasy names like Maren, Kael, or Ash.".to_string()];
    for culture in cultures {
        lines.push(format!(
            "\n## {} — {}",
            culture.name.as_str(),
            culture.description
        ));
        // Show word lists for each slot
        for (slot_name, slot) in &culture.slots {
            if let Some(ref words) = slot.word_list {
                if !words.is_empty() {
                    let sample: Vec<_> = words.iter().take(10).map(|s| s.as_str()).collect();
                    lines.push(format!("  {}: {}", slot_name, sample.join(", ")));
                }
            }
        }
        // Show person name patterns
        if !culture.person_patterns.is_empty() {
            lines.push(format!(
                "  Name patterns: {}",
                culture.person_patterns.join(", ")
            ));
        }
    }
    lines.join("\n")
}
