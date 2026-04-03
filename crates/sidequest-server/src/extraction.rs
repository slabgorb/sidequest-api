//! Text extraction utilities and format conversion for narration processing.
//!
//! Pure functions that parse narrator output for location headers,
//! TTS-clean text, and audio cue conversion.

use sidequest_protocol::{AudioCuePayload, GameMessage};

/// Extract a location name from the first few lines of narration text.
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
    // Simple approach: remove standalone * and _ that look like formatting
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
    // Remove footnote markers [1], [2], etc. — these cause phonemizer
    // word-count mismatches because they aren't natural language tokens.
    // Also remove any bracketed numbers like [12] from narrator output.
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

/// Convert a game-internal AudioCue to a protocol GameMessage for WebSocket broadcast.
///
/// `genre_slug` is prepended to track paths so the client can fetch via `/genre/{slug}/{path}`.
/// `mood` is included so the client's deduplication logic works (it ignores cues without mood).
pub(crate) fn audio_cue_to_game_message(
    cue: &sidequest_game::AudioCue,
    player_id: &str,
    genre_slug: &str,
    mood: Option<&str>,
) -> GameMessage {
    let full_track = cue.track_id.as_ref().map(|path| {
        if genre_slug.is_empty() {
            path.clone()
        } else {
            format!("/genre/{}/{}", genre_slug, path)
        }
    });
    GameMessage::AudioCue {
        payload: AudioCuePayload {
            mood: mood.map(|s| s.to_string()),
            music_track: full_track,
            sfx_triggers: vec![],
            channel: Some(cue.channel.to_string()),
            action: Some(cue.action.to_string()),
            volume: Some(cue.volume),
        },
        player_id: player_id.to_string(),
    }
}
