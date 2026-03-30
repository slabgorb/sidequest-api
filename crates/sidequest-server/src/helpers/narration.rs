//! Narration text processing helpers.

/// Extract a location header from narration text.
///
/// Checks the first 3 non-empty lines for location patterns:
/// - `**Location Name**` (bold header — primary format)
/// - `## Location Name` (markdown h2)
/// - `[Location: Name]` (bracketed tag)
pub(crate) fn extract_location_header(text: &str) -> Option<String> {
    for line in text.lines().take(3) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Bold header: **Location Name**
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return Some(trimmed[2..trimmed.len() - 2].to_string());
        }
        // Markdown h2: ## Location Name
        if trimmed.starts_with("## ") && trimmed.len() > 3 {
            return Some(trimmed[3..].trim().to_string());
        }
        // Bracketed tag: [Location: Name]
        if trimmed.starts_with("[Location:") && trimmed.ends_with(']') {
            let inner = &trimmed[10..trimmed.len() - 1].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
        // Only check the first non-empty line for the primary format,
        // but continue checking for h2/bracketed in lines 2-3.
        break;
    }
    // Second pass: check lines 2-3 for any format (narrator sometimes
    // puts flavor text before the location header)
    for line in text.lines().skip(1).take(2) {
        let trimmed = line.trim();
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return Some(trimmed[2..trimmed.len() - 2].to_string());
        }
        if trimmed.starts_with("## ") && trimmed.len() > 3 {
            return Some(trimmed[3..].trim().to_string());
        }
    }
    None
}

/// Strip the location header line from narration text.
/// Handles all formats recognized by extract_location_header.
pub(crate) fn strip_location_header(text: &str) -> String {
    // Find which line (if any) contains the location header
    for (i, line) in text.lines().take(3).enumerate() {
        let trimmed = line.trim();
        let is_header = (trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4)
            || (trimmed.starts_with("## ") && trimmed.len() > 3)
            || (trimmed.starts_with("[Location:") && trimmed.ends_with(']'));
        if is_header {
            return text
                .lines()
                .enumerate()
                .filter(|(idx, _)| *idx != i)
                .map(|(_, l)| l)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
        }
    }
    text.to_string()
}

/// Bug 5: Extract item acquisitions from narration text.
///
/// Looks for patterns like "you pick up {item}", "you find {item}", "receives {item}", etc.
/// Returns a list of (item_name, item_type) tuples.
pub(crate) fn extract_items_from_narration(text: &str) -> Vec<(String, String)> {
    let text_lower = text.to_lowercase();
    let mut items = Vec::new();

    // Tightened patterns — require 2nd person ("you") to avoid matching
    // NPC dialogue and reported speech.
    let acquisition_patterns = [
        "you pick up ",
        "you find a ",
        "you find an ",
        "you find the ",
        "you found a ",
        "you found an ",
        "you found the ",
        "you acquire ",
        "you take the ",
        "you grab the ",
        "you pocket the ",
        "you loot ",
    ];

    for pattern in &acquisition_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            if start >= text_lower.len() {
                break;
            }
            // Extract the item name: take until punctuation or newline
            let rest = &text[start..];
            let end = rest
                .find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            // Skip if too short
            if item_name.len() >= 3 {
                // Strip leading articles
                let after_article = item_name
                    .strip_prefix("a ")
                    .or_else(|| item_name.strip_prefix("an "))
                    .or_else(|| item_name.strip_prefix("the "))
                    .or_else(|| item_name.strip_prefix("some "))
                    .unwrap_or(item_name)
                    .trim();
                // Truncate at prepositional phrases and adverbs to get clean item names.
                let stop_words = [
                    " with ",
                    " from ",
                    " into ",
                    " onto ",
                    " against ",
                    " across ",
                    " along ",
                    " through ",
                    " around ",
                    " behind ",
                    " before ",
                    " after ",
                    " again",
                    " as ",
                    " and then",
                    " while ",
                    " that ",
                    " which ",
                ];
                let mut clean_end = after_article.len();
                for sw in &stop_words {
                    if let Some(pos) = after_article.to_lowercase().find(sw) {
                        if pos > 0 && pos < clean_end {
                            clean_end = pos;
                        }
                    }
                }
                // Also cap at 4 words max for item names
                let words: Vec<&str> = after_article[..clean_end].split_whitespace().collect();
                let clean_name = if words.len() > 4 {
                    words[..4].join(" ")
                } else {
                    words.join(" ")
                };
                if clean_name.len() >= 2 {
                    // Simple category heuristic
                    let lower_name = clean_name.to_lowercase();
                    let category = if lower_name.contains("sword")
                        || lower_name.contains("blade")
                        || lower_name.contains("axe")
                        || lower_name.contains("dagger")
                        || lower_name.contains("weapon")
                    {
                        "weapon"
                    } else if lower_name.contains("armor")
                        || lower_name.contains("shield")
                        || lower_name.contains("helmet")
                        || lower_name.contains("plate")
                    {
                        "armor"
                    } else if lower_name.contains("potion")
                        || lower_name.contains("salve")
                        || lower_name.contains("herb")
                        || lower_name.contains("food")
                        || lower_name.contains("drink")
                    {
                        "consumable"
                    } else if lower_name.contains("key")
                        || lower_name.contains("tool")
                        || lower_name.contains("rope")
                        || lower_name.contains("torch")
                        || lower_name.contains("lantern")
                    {
                        "tool"
                    } else if lower_name.contains("coin")
                        || lower_name.contains("gem")
                        || lower_name.contains("gold")
                        || lower_name.contains("jewel")
                    {
                        "treasure"
                    } else {
                        "misc"
                    };
                    items.push((clean_name.to_string(), category.to_string()));
                }
            }
            search_from = start;
        }
    }

    items
}

/// Strip markdown syntax from text for TTS voice synthesis.
/// Removes bold (**), italic (*/_), headers (#), links, images, code blocks,
/// and footnote markers ([1], [2], etc.) that cause phonemizer word-count mismatches.
pub(crate) fn strip_markdown_for_tts(text: &str) -> String {
    let mut result = text.to_string();
    // Bold and italic: **text**, *text*, __text__, _text_
    // Process ** before * to avoid partial matches
    result = result.replace("**", "");
    result = result.replace("__", "");
    // Single * and _ as italic markers (only between word boundaries)
    let mut cleaned = String::with_capacity(result.len());
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == '*' || chars[i] == '_')
            && i + 1 < chars.len()
            && chars[i + 1].is_alphanumeric()
        {
            // Skip opening italic marker
            i += 1;
            continue;
        }
        if (chars[i] == '*' || chars[i] == '_') && i > 0 && chars[i - 1].is_alphanumeric() {
            // Skip closing italic marker
            i += 1;
            continue;
        }
        cleaned.push(chars[i]);
        i += 1;
    }
    // Remove markdown headers (# at start of line)
    cleaned = cleaned
        .lines()
        .map(|line| line.trim_start_matches('#').trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    // Remove footnote markers [1], [2], etc.
    let mut tts_clean = String::with_capacity(cleaned.len());
    let clean_chars: Vec<char> = cleaned.chars().collect();
    let mut j = 0;
    while j < clean_chars.len() {
        if clean_chars[j] == '[' {
            // Look ahead for a closing bracket with only digits inside
            if let Some(close) = clean_chars[j + 1..].iter().position(|&c| c == ']') {
                let inside = &clean_chars[j + 1..j + 1 + close];
                if !inside.is_empty() && inside.iter().all(|c| c.is_ascii_digit()) {
                    // Skip the entire [N] marker
                    j += close + 2; // skip past ']'
                    continue;
                }
            }
        }
        tts_clean.push(clean_chars[j]);
        j += 1;
    }
    // Collapse any double-spaces left by removed markers
    while tts_clean.contains("  ") {
        tts_clean = tts_clean.replace("  ", " ");
    }
    tts_clean.trim().to_string()
}

/// Extract item losses from narration — trades, gifts, drops.
/// Returns a list of item names that the player lost.
pub(crate) fn extract_item_losses(text: &str) -> Vec<String> {
    let text_lower = text.to_lowercase();
    let mut lost = Vec::new();

    let loss_patterns = [
        "hand over ",
        "hands over ",
        "give away ",
        "gives away ",
        "trade the ",
        "trades the ",
        "trading the ",
        "hand the ",
        "hands the ",
        "surrender the ",
        "surrenders the ",
        "drop the ",
        "drops the ",
        "toss the ",
        "tosses the ",
        "you give ",
        "you hand ",
        "you trade ",
        "you surrender ",
        "you drop ",
        "you toss ",
        "parts with the ",
        "part with the ",
        "relinquish the ",
        "relinquishes the ",
    ];

    for pattern in &loss_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            if start >= text_lower.len() {
                break;
            }
            let rest = &text[start..];
            let end = rest
                .find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            if item_name.len() >= 2 && item_name.len() <= 60 {
                let after_article = item_name
                    .strip_prefix("a ")
                    .or_else(|| item_name.strip_prefix("an "))
                    .or_else(|| item_name.strip_prefix("the "))
                    .or_else(|| item_name.strip_prefix("some "))
                    .unwrap_or(item_name)
                    .trim();
                // Truncate at prepositions
                let stop_words = [" to ", " for ", " in ", " with ", " from ", " as "];
                let mut clean_end = after_article.len();
                for sw in &stop_words {
                    if let Some(p) = after_article.to_lowercase().find(sw) {
                        if p > 0 && p < clean_end {
                            clean_end = p;
                        }
                    }
                }
                let words: Vec<&str> = after_article[..clean_end].split_whitespace().collect();
                let clean_name = if words.len() > 4 {
                    words[..4].join(" ")
                } else {
                    words.join(" ")
                };
                if clean_name.len() >= 2 {
                    lost.push(clean_name);
                }
            }
            search_from = start;
        }
    }

    lost
}
