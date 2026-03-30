//! NPC registry extraction and context building.

/// NPC registry entry — re-exported from sidequest-game for persistence.
pub(crate) type NpcRegistryEntry = sidequest_game::NpcRegistryEntry;

/// Extract NPC names from narration text and update the registry.
/// Looks for patterns like dialogue attribution ("Name says", "Name asks")
/// and introduction patterns ("a woman named Name", "Name, the blacksmith").
pub(crate) fn update_npc_registry(
    registry: &mut Vec<NpcRegistryEntry>,
    narration: &str,
    current_location: &str,
    turn_count: u32,
    location_names: &[&str],
) {
    // Build a set of names that should never become NPCs (location/region names).
    let rejected: Vec<String> = std::iter::once(current_location)
        .chain(location_names.iter().copied())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    let is_location_name = |name: &str| -> bool {
        let lower = name.to_lowercase();
        rejected
            .iter()
            .any(|loc| lower == *loc || loc.contains(&lower) || lower.contains(loc.as_str()))
    };

    // Common English words that should never be NPC names.
    const COMMON_WORDS: &[&str] = &[
        "the", "a", "an", "it", "its", "this", "that", "these", "those",
        "there", "here", "then", "now", "but", "and", "or", "yet", "so",
        "you", "your", "my", "our", "their", "his", "her", "we", "they",
        "she", "he", "him", "hers", "herself", "himself", "themselves", "itself",
        "i", "me", "us", "them", "who", "whom", "whose", "what", "which",
        "something", "someone", "somebody", "nothing", "nobody", "anyone",
        "anything", "everything", "everyone", "one", "each", "every",
        "another", "other", "others", "both", "few", "many", "some",
        "after", "before", "above", "below", "behind", "between",
        "perhaps", "maybe", "also", "still", "just", "only", "even",
        "soon", "once", "when", "where", "while", "since", "until",
        "though", "although", "however", "meanwhile", "suddenly",
        "slowly", "quickly", "finally", "somehow",
    ];
    let is_common_word = |name: &str| -> bool {
        let lower = name.to_lowercase();
        if !name.contains(' ') {
            return COMMON_WORDS.contains(&lower.as_str());
        }
        if let Some(first) = lower.split_whitespace().next() {
            if COMMON_WORDS.contains(&first) {
                return true;
            }
        }
        false
    };

    // Dialogue attribution: "Name says/asks/replies/shouts/whispers/mutters"
    let speech_verbs = [
        "says", "asks", "replies", "shouts", "whispers", "mutters", "growls", "calls", "declares",
        "speaks",
    ];
    let text_lower = narration.to_lowercase();

    for verb in &speech_verbs {
        let pattern = format!(" {}", verb);
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(&pattern) {
            let abs_pos = search_from + pos;
            let before = &narration[..abs_pos];
            let name_start = before
                .rfind(|c: char| matches!(c, '.' | '!' | '?' | '\n' | '"' | '\u{201c}'))
                .map(|i| i + 1)
                .unwrap_or(0);
            let candidate = before[name_start..].trim();
            if candidate.len() >= 2
                && candidate.len() <= 40
                && candidate.chars().next().map_or(false, |c| c.is_uppercase())
            {
                let name = candidate.to_string();
                if !is_common_word(&name)
                    && !is_location_name(&name)
                {
                    let name_lower = name.to_lowercase();
                    if let Some(entry) = registry.iter_mut().find(|e| {
                        e.name == name
                            || e.name.to_lowercase().contains(&name_lower)
                            || name_lower.contains(&e.name.to_lowercase())
                    }) {
                        entry.last_seen_turn = turn_count;
                        if !current_location.is_empty() {
                            entry.location = current_location.to_string();
                        }
                        if name.len() > entry.name.len() {
                            entry.name = name;
                        }
                    } else {
                        registry.push(NpcRegistryEntry {
                            name,
                            pronouns: String::new(),
                            role: String::new(),
                            location: current_location.to_string(),
                            last_seen_turn: turn_count,
                            age: String::new(),
                            appearance: String::new(),
                        });
                    }
                }
            }
            search_from = abs_pos + 1;
        }
    }

    // Introduction patterns: "a woman named X", "a man called X", "named X", "called X"
    let intro_patterns = ["named ", "called ", "known as "];
    for pat in &intro_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pat) {
            let abs_pos = search_from + pos + pat.len();
            if abs_pos < narration.len() {
                let rest = &narration[abs_pos..];
                let name_end = rest
                    .find(|c: char| {
                        matches!(c, ',' | '.' | '!' | '?' | ';' | '\n' | '"' | '\u{201d}')
                    })
                    .unwrap_or(rest.len());
                let candidate = rest[..name_end].trim();
                if candidate.len() >= 2
                    && candidate.len() <= 40
                    && candidate.chars().next().map_or(false, |c| c.is_uppercase())
                {
                    let name = candidate.to_string();
                    if !is_common_word(&name) && !is_location_name(&name) {
                        if !registry.iter().any(|e| e.name == name) {
                            let role = if name_end < rest.len() {
                                let after_name = &rest[name_end..];
                                if after_name.starts_with(", the ")
                                    || after_name.starts_with(", a ")
                                {
                                    let role_start =
                                        after_name.find(' ').map(|i| i + 1).unwrap_or(0);
                                    let role_text = &after_name[role_start..];
                                    let role_end = role_text
                                        .find(|c: char| matches!(c, ',' | '.' | '!' | '?'))
                                        .unwrap_or(role_text.len().min(40));
                                    role_text[..role_end].trim().to_string()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };
                            registry.push(NpcRegistryEntry {
                                name,
                                pronouns: String::new(),
                                role,
                                location: current_location.to_string(),
                                last_seen_turn: turn_count,
                                age: String::new(),
                                appearance: String::new(),
                            });
                        }
                    }
                }
            }
            search_from = abs_pos;
        }
    }

    // Appositive pattern: "Name, the blacksmith" / "Name, a merchant"
    {
        use regex::Regex;
        let appos_re =
            Regex::new(r"\b([A-Z][a-z]+(?:\s[A-Z][a-z]+)?), (?:the|a|an) ([a-z][a-z ]{1,30})")
                .unwrap();
        for caps in appos_re.captures_iter(narration) {
            let name = caps[1].to_string();
            let role = caps[2]
                .trim_end_matches(|c: char| matches!(c, ',' | '.' | '!' | '?'))
                .trim()
                .to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    if entry.role.is_empty() && !role.is_empty() {
                        entry.role = role;
                    }
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role,
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Proper nouns as sentence subjects
    {
        use regex::Regex;
        let subject_re = Regex::new(r"(?:^|[.!?]\s+)([A-Z][a-z]+(?:\s[A-Z][a-z]+)?)\s+(?:is|was|has|had|walks|stands|sits|looks|turns|nods|shakes|moves|steps|reaches|pulls|holds|places|waves|smiles|frowns|laughs|sighs|watches|leads|appears|enters|exits|approaches|stares|glances|points|gestures|offers|hands|grabs|takes|gives|opens|closes|runs|stops|begins|continues)\b").unwrap();
        for caps in subject_re.captures_iter(narration) {
            let name = caps[1].to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role: String::new(),
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Possessive form: "Name's"
    {
        use regex::Regex;
        let poss_re = Regex::new(r"\b([A-Z][a-z]+(?:\s[A-Z][a-z]+)?)'s\b").unwrap();
        for caps in poss_re.captures_iter(narration) {
            let name = caps[1].to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role: String::new(),
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Infer pronouns from narration context
    for entry in registry.iter_mut() {
        if !entry.pronouns.is_empty() {
            continue;
        }
        let name_lower = entry.name.to_lowercase();
        if let Some(name_pos) = text_lower.find(&name_lower) {
            let after = &text_lower[name_pos..];
            let window = &after[..after.len().min(200)];
            if window.contains(" she ") || window.contains(" her ") || window.contains(" hers ") {
                entry.pronouns = "she/her".to_string();
            } else if window.contains(" he ")
                || window.contains(" his ")
                || window.contains(" him ")
            {
                entry.pronouns = "he/him".to_string();
            } else if window.contains(" they ")
                || window.contains(" their ")
                || window.contains(" them ")
            {
                entry.pronouns = "they/them".to_string();
            }
        }
        if entry.pronouns.is_empty() {
            entry.pronouns = "they/them".to_string();
        }
    }
}

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
        if !entry.location.is_empty() {
            desc.push_str(&format!(" — at {}", entry.location));
        }
        lines.push(desc);
    }
    lines.join("\n")
}

/// Build a name bank context string from genre pack cultures for the narrator prompt.
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
        for (slot_name, slot) in &culture.slots {
            if let Some(ref words) = slot.word_list {
                if !words.is_empty() {
                    let sample: Vec<_> = words.iter().take(10).map(|s| s.as_str()).collect();
                    lines.push(format!("  {}: {}", slot_name, sample.join(", ")));
                }
            }
        }
        if !culture.person_patterns.is_empty() {
            lines.push(format!(
                "  Name patterns: {}",
                culture.person_patterns.join(", ")
            ));
        }
    }
    lines.join("\n")
}
