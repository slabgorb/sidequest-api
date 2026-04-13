//! Text extraction utilities and format conversion for narration processing.
//!
//! Pure functions that parse narrator output for location headers,
//! narration display cleanup (stage direction stripping, punctuation
//! normalization), and audio cue conversion.

use regex::Regex;
use std::sync::LazyLock;

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

// ---------------------------------------------------------------------------
// Combat bracket patterns — compiled once, reused forever
// ---------------------------------------------------------------------------

/// Matches lines that are entirely a bracketed combat/mechanical annotation.
/// Examples:
///   [COMBAT: Riissor the Rotting — Synth (HP 12) | Kael HP 8/8]
///   [Kael's charge — first strike lands. Riissor takes 2 damage. HP: 10/12]
///   [Combat Round 3]
///   [Initiative: Kael, Riissor]
///   [HP: 10/12]
///
/// Pattern: a line whose trimmed content starts with `[` and ends with `]`,
/// AND contains at least one combat keyword (COMBAT, HP, damage, round, initiative,
/// strike, attack, defend, takes, deals).
static COMBAT_BRACKET_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*\[(?:[^\]]*(?:COMBAT|HP[:\s]|damage|round|initiative|strike|attack|defend|takes\s+\d|deals\s+\d)[^\]]*)\]\s*$"
    ).unwrap()
});

/// Strip bracketed combat/mechanical annotations from narration text.
///
/// The narrator LLM sometimes emits inline bracketed lines like
/// `[COMBAT: Enemy — Type (HP 12) | Player HP 8/8]` or
/// `[Player's charge — first strike lands. Enemy takes 2 damage. HP: 10/12]`
/// mixed into prose. These are mechanical data that belongs in the CombatOverlay
/// (delivered via CombatEvent messages), not in narration text.
///
/// Strips entire lines that match the combat bracket pattern, then collapses
/// any resulting double-blank-lines.
pub(crate) fn strip_combat_brackets(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut stripped_count = 0u32;

    for line in text.lines() {
        if COMBAT_BRACKET_LINE.is_match(line) {
            stripped_count += 1;
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }

    if stripped_count > 0 {
        tracing::debug!(stripped_count, "combat_brackets.stripped from narration");
    }

    // Collapse multiple blank lines left by removed brackets
    let mut collapsed = String::with_capacity(result.len());
    let mut blank_count = 0;
    for line in result.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                collapsed.push('\n');
            }
        } else {
            blank_count = 0;
            collapsed.push_str(line);
            collapsed.push('\n');
        }
    }

    collapsed.trim().to_string()
}

/// Strip fenced code blocks from narration text (e.g. ```game_patch ... ```).
///
/// The narrator emits structured JSON blocks (game_patch, etc.) inline with prose.
/// These must be removed before the narration reaches the client for display.
/// Returns the text with all fenced blocks removed and whitespace normalized.
pub(crate) fn strip_fenced_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut inside_fence = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if !inside_fence && trimmed.starts_with("```") {
            inside_fence = true;
            continue;
        }
        if inside_fence {
            if trimmed == "```" {
                inside_fence = false;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }

    // Collapse multiple blank lines left by removed blocks
    let mut collapsed = String::with_capacity(result.len());
    let mut blank_count = 0;
    for line in result.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                collapsed.push('\n');
            }
        } else {
            blank_count = 0;
            collapsed.push_str(line);
            collapsed.push('\n');
        }
    }

    collapsed.trim().to_string()
}

/// Strip fourth-wall-breaking meta-questions and LLM self-referential reasoning
/// from narrator output.
///
/// Defensive guardrail: if the LLM breaks character and asks the player about game
/// mechanics, genre, system, or its own constraints, remove those paragraphs.
/// Fix: playtest-2026-04-05 — narrator asked "What genre is Ashgate Square in?"
/// Fix: playtest-2026-04-12 — narrator meta-commentary ("I notice the genre isn't
///   specified...") leaked into rendered narration.
pub(crate) fn strip_fourth_wall(text: &str) -> String {
    // Patterns that indicate the narrator is breaking character to ask about
    // game mechanics, genre identity, or its own system prompt.
    const FOURTH_WALL_PATTERNS: &[&str] = &[
        "what genre",
        "what system",
        "what setting",
        "what game",
        "what rpg",
        "what ruleset",
        "which genre",
        "which system",
        "which setting",
        "which game",
        "i need to know",
        "i need more context",
        "i need more information about the",
        "could you tell me what",
        "can you clarify what",
        "what kind of game",
        "what type of game",
        "please specify",
        "what would you like the genre",
        "i'm not sure what genre",
        "i don't know what genre",
    ];

    // Paragraph-start patterns for LLM self-referential meta-commentary.
    // These only match when the paragraph STARTS with the pattern AND contains
    // a meta-keyword — this avoids stripping in-character NPC dialogue like
    // "I notice the torches have gone out."
    const META_STARTERS: &[&str] = &[
        "i notice",
        "i should",
        "i need to",
        "i'll ",
        "i want to make sure",
        "let me ",
    ];

    // Meta-keywords that confirm a paragraph with a META_STARTER is actually
    // LLM reasoning rather than in-character narration.
    const META_KEYWORDS: &[&str] = &[
        "genre",
        "genre pack",
        "the field",
        "is blank",
        "isn't specified",
        "not specified",
        "your request",
        "system prompt",
        "specified",
        "json",
        "game_patch",
        "narrator",
        "as an ai",
        "language model",
        "instruction",
    ];

    // System terminology that should NEVER appear in player-facing narration.
    // These are internal engine terms the narrator prompt uses for navigation
    // context — if they leak into prose, the line is stripped unconditionally.
    // Fix: playtest-2026-04-12 — narrator echoed "room graph" in prose.
    const SYSTEM_TERMS: &[&str] = &[
        "room graph",
        "room_graph",
        "room navigation",
        "mapped exit",
        "mapped_exit",
        "exit target",
        "room_id",
        "room id",
        "unmapped exit",
        "navigation graph",
    ];

    // Three-pass stripping:
    // 1. Line-level: original fourth-wall patterns (removes individual lines)
    // 2. Paragraph-level: meta-commentary (starter + keyword on paragraph starts)
    // 3. Line-level: system terminology (removes lines with engine internals)

    // --- Pass 1: line-level fourth-wall stripping (preserves backward compat) ---
    let mut pass1_lines: Vec<&str> = Vec::new();
    let mut stripped_any = false;

    for line in text.lines() {
        let lower = line.to_lowercase();
        let is_fourth_wall = FOURTH_WALL_PATTERNS
            .iter()
            .any(|pattern| lower.contains(pattern));
        if is_fourth_wall {
            stripped_any = true;
            tracing::warn!(
                stripped_text = %line,
                reason = "fourth_wall_break",
                "narrator.fourth_wall_stripped"
            );
        } else {
            pass1_lines.push(line);
        }
    }

    let after_pass1 = pass1_lines.join("\n");

    // --- Pass 2: paragraph-level meta-commentary stripping ---
    // Split on paragraph boundaries (double newline). A META_STARTER at the
    // start of a paragraph combined with a META_KEYWORD anywhere in it
    // indicates LLM self-referential reasoning, not in-character narration.
    let paragraphs: Vec<&str> = after_pass1.split("\n\n").collect();
    let mut kept_paragraphs: Vec<&str> = Vec::new();

    for para in &paragraphs {
        let trimmed_para = para.trim();
        if trimmed_para.is_empty() {
            continue;
        }
        let lower = trimmed_para.to_lowercase();

        let has_meta_starter = META_STARTERS
            .iter()
            .any(|starter| lower.starts_with(starter));
        let has_meta_keyword = META_KEYWORDS.iter().any(|keyword| lower.contains(keyword));

        if has_meta_starter && has_meta_keyword {
            stripped_any = true;
            tracing::warn!(
                stripped_text = %trimmed_para.chars().take(120).collect::<String>(),
                reason = "meta_commentary",
                "narrator.fourth_wall_stripped"
            );
        } else {
            kept_paragraphs.push(para);
        }
    }

    // --- Pass 3: line-level system terminology stripping ---
    // Engine-internal terms should never appear in player-facing prose.
    let after_pass2 = kept_paragraphs.join("\n\n");
    let mut pass3_lines: Vec<&str> = Vec::new();
    for line in after_pass2.lines() {
        let lower = line.to_lowercase();
        let has_system_term = SYSTEM_TERMS.iter().any(|term| lower.contains(term));
        if has_system_term {
            stripped_any = true;
            tracing::warn!(
                stripped_text = %line,
                reason = "system_terminology",
                "narrator.fourth_wall_stripped"
            );
        } else {
            pass3_lines.push(line);
        }
    }

    if stripped_any {
        let result = pass3_lines.join("\n");
        let trimmed = result.trim();
        if trimmed.is_empty() {
            "You look around, taking in your surroundings.".to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        text.to_string()
    }
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
            music_volume: None,
            sfx_volume: None,
            voice_volume: None,
            crossfade_ms: None,
        },
        player_id: player_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_combat_brackets_removes_combat_header() {
        let input = "The creature lunges forward.\n\
                      [COMBAT: Riissor the Rotting — Synth (HP 12) | Kael HP 8/8]\n\
                      You barely dodge the blow.";
        let result = strip_combat_brackets(input);
        assert_eq!(
            result,
            "The creature lunges forward.\nYou barely dodge the blow."
        );
    }

    #[test]
    fn strip_combat_brackets_removes_damage_line() {
        let input = "Your blade connects with a sickening crack.\n\
                      [Kael's charge — first strike lands. Riissor takes 2 damage. HP: 10/12]\n\
                      The creature staggers backward.";
        let result = strip_combat_brackets(input);
        assert_eq!(
            result,
            "Your blade connects with a sickening crack.\nThe creature staggers backward."
        );
    }

    #[test]
    fn strip_combat_brackets_removes_round_marker() {
        let input = "[Combat Round 3]\nSwords clash in the dim hallway.";
        let result = strip_combat_brackets(input);
        assert_eq!(result, "Swords clash in the dim hallway.");
    }

    #[test]
    fn strip_combat_brackets_preserves_non_combat_brackets() {
        // Inline brackets (not on their own line) are preserved
        let input = "He said [something odd] and walked away.\n\
                      The [ancient rune] glowed faintly.";
        let result = strip_combat_brackets(input);
        assert_eq!(
            result,
            "He said [something odd] and walked away.\nThe [ancient rune] glowed faintly."
        );
    }

    #[test]
    fn strip_combat_brackets_preserves_dialogue_with_brackets() {
        let input = "\"[I'll never surrender,]\" she growled.";
        let result = strip_combat_brackets(input);
        assert_eq!(result, "\"[I'll never surrender,]\" she growled.");
    }

    #[test]
    fn strip_combat_brackets_removes_hp_only_line() {
        let input = "Blood drips from the wound.\n[HP: 6/12]\nYou press on.";
        let result = strip_combat_brackets(input);
        assert_eq!(result, "Blood drips from the wound.\nYou press on.");
    }

    #[test]
    fn strip_combat_brackets_removes_initiative_line() {
        let input = "[Initiative: Kael, Riissor, Brenna]\nThe fight begins.";
        let result = strip_combat_brackets(input);
        assert_eq!(result, "The fight begins.");
    }

    #[test]
    fn strip_combat_brackets_no_change_when_clean() {
        let input = "The tavern is warm and inviting.\nA bard plays softly in the corner.";
        let result = strip_combat_brackets(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_combat_brackets_whole_line_non_combat_bracket_preserved() {
        // A line that IS a bracket but has no combat keywords is preserved
        let input = "[The ancient prophecy speaks of a chosen one]\nYou continue reading.";
        let result = strip_combat_brackets(input);
        assert_eq!(
            result,
            "[The ancient prophecy speaks of a chosen one]\nYou continue reading."
        );
    }

    // === strip_fourth_wall tests ===

    #[test]
    fn strip_fourth_wall_removes_genre_question() {
        let input = "**Ashgate Square**\nThe cobblestones are slick with rain.\nWhat genre is Ashgate Square in?\nYou hear footsteps behind you.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, "**Ashgate Square**\nThe cobblestones are slick with rain.\nYou hear footsteps behind you.");
    }

    #[test]
    fn strip_fourth_wall_removes_system_question() {
        let input = "I need to know what system we're using before I can narrate combat.";
        let result = strip_fourth_wall(input);
        // Everything stripped — fallback
        assert_eq!(result, "You look around, taking in your surroundings.");
    }

    #[test]
    fn strip_fourth_wall_preserves_clean_narration() {
        let input = "The tavern is warm and inviting.\nA bard plays softly in the corner.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_fourth_wall_case_insensitive() {
        let input =
            "The road stretches ahead.\nWhat Genre are we playing?\nDust swirls at your feet.";
        let result = strip_fourth_wall(input);
        assert_eq!(
            result,
            "The road stretches ahead.\nDust swirls at your feet."
        );
    }

    #[test]
    fn strip_fourth_wall_removes_multiple_breaks() {
        let input = "Could you tell me what setting this is?\nI need more context about the world.\nThe fog rolls in.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, "The fog rolls in.");
    }

    // === meta-commentary (pass 2) tests ===

    #[test]
    fn strip_fourth_wall_removes_meta_commentary_i_notice() {
        let input = "The cavern drips with moisture.\n\nI notice the genre isn't specified in the configuration. Let me continue with a fantasy setting.\n\nYou press deeper into the tunnel.";
        let result = strip_fourth_wall(input);
        assert_eq!(
            result,
            "The cavern drips with moisture.\n\nYou press deeper into the tunnel."
        );
    }

    #[test]
    fn strip_fourth_wall_removes_let_me_meta() {
        let input = "Let me check the narrator instructions before continuing.\n\nThe torchlight flickers against the damp stone walls.";
        let result = strip_fourth_wall(input);
        assert_eq!(
            result,
            "The torchlight flickers against the damp stone walls."
        );
    }

    #[test]
    fn strip_fourth_wall_preserves_in_character_i_notice() {
        // "I notice" without meta-keywords should NOT be stripped —
        // it's valid in-character NPC dialogue.
        let input = "\"I notice the torches have gone out,\" the dwarf mutters.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_fourth_wall_removes_i_should_meta() {
        let input = "I should note that the genre pack doesn't specify this region.\n\nThe merchant waves you over.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, "The merchant waves you over.");
    }

    #[test]
    fn strip_fourth_wall_all_meta_fallback() {
        let input = "I notice the genre isn't specified. I'll assume fantasy.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, "You look around, taking in your surroundings.");
    }

    // === system terminology (pass 3) tests ===

    #[test]
    fn strip_fourth_wall_removes_room_graph_reference() {
        let input = "The corridor splits ahead.\nYou follow the room graph northward.\nTorchlight flickers on damp stone.";
        let result = strip_fourth_wall(input);
        assert_eq!(
            result,
            "The corridor splits ahead.\nTorchlight flickers on damp stone."
        );
    }

    #[test]
    fn strip_fourth_wall_removes_mapped_exit() {
        let input = "The mapped exit leads to the eastern tower.\nA cold wind greets you.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, "A cold wind greets you.");
    }

    #[test]
    fn strip_fourth_wall_preserves_normal_exit_mention() {
        let input = "You see an exit to the north, partially blocked by rubble.";
        let result = strip_fourth_wall(input);
        assert_eq!(result, input);
    }
}
