//! Story 4-7: TTS text segmentation — break narration into speakable segments
//!
//! RED phase — these tests exercise SentenceSegmenter for splitting narration
//! text into sentence-level chunks suitable for streaming TTS delivery.
//! They will fail until Dev implements:
//!   - segmenter.rs: SentenceSegmenter::segment() method
//!   - Abbreviation handling (Mr., Dr., etc. don't split)
//!   - Ellipsis, exclamation, question mark boundaries
//!   - Quoted speech preservation
//!   - Edge cases (empty, whitespace, single sentence, no terminal punct)

use sidequest_game::segmenter::{Segment, SentenceSegmenter};

// ============================================================================
// Test fixtures
// ============================================================================

fn segmenter() -> SentenceSegmenter {
    SentenceSegmenter::new()
}

// ============================================================================
// AC-1: Basic sentence splitting
// ============================================================================

#[test]
fn single_sentence_returns_one_segment() {
    let result = segmenter().segment("The warrior draws his sword.");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "The warrior draws his sword.");
}

#[test]
fn two_sentences_split_correctly() {
    let result = segmenter().segment("The warrior draws his sword. The enemy approaches.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "The warrior draws his sword.");
    assert_eq!(result[1].text, "The enemy approaches.");
}

#[test]
fn three_sentences_preserves_order() {
    let result = segmenter().segment(
        "The gate opens. Wind howls through the courtyard. A shadow moves in the darkness.",
    );
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].text, "The gate opens.");
    assert_eq!(result[1].text, "Wind howls through the courtyard.");
    assert_eq!(result[2].text, "A shadow moves in the darkness.");
}

// ============================================================================
// AC-2: Abbreviation handling — must NOT split on abbreviations
// ============================================================================

#[test]
fn mr_abbreviation_does_not_split() {
    let result = segmenter().segment("Mr. Smith entered the tavern. He ordered a drink.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Mr. Smith entered the tavern.");
    assert_eq!(result[1].text, "He ordered a drink.");
}

#[test]
fn dr_abbreviation_does_not_split() {
    let result = segmenter().segment("Dr. Blackwood examined the wound. It was deep.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Dr. Blackwood examined the wound.");
    assert_eq!(result[1].text, "It was deep.");
}

#[test]
fn multiple_abbreviations_in_sequence() {
    let result =
        segmenter().segment("Gen. Holt spoke with Lt. Col. Graves about the mission. They agreed.");
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("Gen."));
    assert!(result[0].text.contains("Lt."));
    assert!(result[0].text.contains("Col."));
}

#[test]
fn mrs_abbreviation_does_not_split() {
    let result = segmenter().segment("Mrs. Chen prepared the ritual. The candles flickered.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Mrs. Chen prepared the ritual.");
}

#[test]
fn etc_abbreviation_does_not_split() {
    let result = segmenter().segment("He carried swords, shields, etc. The load was heavy.");
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("etc."));
}

// ============================================================================
// AC-3: Exclamation and question marks
// ============================================================================

#[test]
fn exclamation_mark_splits() {
    let result = segmenter().segment("Watch out! The ceiling is collapsing!");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Watch out!");
    assert_eq!(result[1].text, "The ceiling is collapsing!");
}

#[test]
fn question_mark_splits() {
    let result = segmenter().segment("Who goes there? State your name.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Who goes there?");
    assert_eq!(result[1].text, "State your name.");
}

#[test]
fn mixed_terminal_punctuation() {
    let result = segmenter().segment("The beast roared! Did you hear that? We need to run.");
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].text, "The beast roared!");
    assert_eq!(result[1].text, "Did you hear that?");
    assert_eq!(result[2].text, "We need to run.");
}

// ============================================================================
// AC-4: Ellipsis handling
// ============================================================================

#[test]
fn ellipsis_followed_by_capital_splits() {
    let result = segmenter().segment("The voice faded... A new light appeared.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "The voice faded...");
    assert_eq!(result[1].text, "A new light appeared.");
}

#[test]
fn unicode_ellipsis_splits() {
    let result = segmenter().segment("The trail went cold\u{2026} Something moved ahead.");
    assert_eq!(result.len(), 2);
    assert!(result[0].text.ends_with('\u{2026}'));
}

#[test]
fn ellipsis_without_capital_does_not_split() {
    // "..." followed by lowercase should stay as one segment — continuation
    let result = segmenter().segment("The darkness grew... and grew some more.");
    assert_eq!(result.len(), 1);
}

// ============================================================================
// AC-5: Quoted speech
// ============================================================================

#[test]
fn quoted_sentence_stays_together() {
    let result = segmenter().segment(r#""You shall not pass!" The wizard slammed his staff down."#);
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("You shall not pass!"));
    assert!(result[1].text.contains("The wizard"));
}

#[test]
fn smart_quotes_handled() {
    let result = segmenter().segment("\u{201c}Run!\u{201d} The captain shouted the order.");
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("Run!"));
}

#[test]
fn period_inside_closing_quote_splits() {
    let result = segmenter().segment(r#""The bridge is gone." She turned away."#);
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("The bridge is gone."));
    assert!(result[1].text.contains("She turned away."));
}

// ============================================================================
// AC-6: Edge cases
// ============================================================================

#[test]
fn empty_string_returns_empty_vec() {
    let result = segmenter().segment("");
    assert!(result.is_empty());
}

#[test]
fn whitespace_only_returns_empty_vec() {
    let result = segmenter().segment("   \n\t  ");
    assert!(result.is_empty());
}

#[test]
fn no_terminal_punctuation_returns_single_segment() {
    let result = segmenter().segment("The ancient door creaked open");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "The ancient door creaked open");
}

#[test]
fn trailing_whitespace_trimmed() {
    let result = segmenter().segment("   Hello world.   Goodbye.   ");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Hello world.");
    assert_eq!(result[1].text, "Goodbye.");
}

#[test]
fn multiple_spaces_between_sentences_handled() {
    let result = segmenter().segment("First sentence.    Second sentence.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "First sentence.");
    assert_eq!(result[1].text, "Second sentence.");
}

#[test]
fn newlines_between_sentences_handled() {
    let result = segmenter().segment("First sentence.\nSecond sentence.\nThird.");
    assert_eq!(result.len(), 3);
}

// ============================================================================
// Segment metadata
// ============================================================================

#[test]
fn segments_have_sequential_indices() {
    let result = segmenter().segment("One. Two. Three.");
    assert_eq!(result[0].index, 0);
    assert_eq!(result[1].index, 1);
    assert_eq!(result[2].index, 2);
}

#[test]
fn segments_track_byte_offsets() {
    let text = "Hello. World.";
    let result = segmenter().segment(text);
    assert_eq!(result.len(), 2);
    // First segment starts at byte 0
    assert_eq!(result[0].byte_offset, 0);
    // Second segment starts after "Hello. " (7 bytes)
    assert!(result[1].byte_offset > 0);
    // Verify the offset points to the right text
    assert_eq!(&text[result[1].byte_offset..].trim_start()[..6], "World.");
}

#[test]
fn segment_is_last_flag() {
    let result = segmenter().segment("First. Second. Third.");
    assert!(!result[0].is_last);
    assert!(!result[1].is_last);
    assert!(result[2].is_last);
}

#[test]
fn single_segment_is_last() {
    let result = segmenter().segment("Only one sentence.");
    assert_eq!(result.len(), 1);
    assert!(result[0].is_last);
}

// ============================================================================
// Narrative-specific patterns (game narration text)
// ============================================================================

#[test]
fn dialogue_attribution_stays_with_speech() {
    // "said X" after a quoted sentence should attach to the quote
    let result =
        segmenter().segment(r#""We must retreat," said the commander. The troops fell back."#);
    assert_eq!(result.len(), 2);
    assert!(result[0].text.contains("said the commander"));
}

#[test]
fn long_narration_segments_correctly() {
    let narration = "The ancient fortress loomed ahead, its towers piercing the storm clouds. \
        Lightning illuminated the crumbling walls. \
        Inside, the sound of dripping water echoed through empty halls. \
        Something stirred in the shadows.";
    let result = segmenter().segment(narration);
    assert_eq!(result.len(), 4);
}

#[test]
fn combat_narration_with_mixed_punctuation() {
    let narration = "Grak swings his axe! The blow connects, dealing massive damage. \
        The enemy staggers back... Can it survive another hit?";
    let result = segmenter().segment(narration);
    assert_eq!(result.len(), 4);
    assert!(result[0].text.ends_with('!'));
    assert!(result[1].text.ends_with('.'));
    assert!(result[2].text.ends_with("..."));
    assert!(result[3].text.ends_with('?'));
}

// ============================================================================
// Parity with Python SentenceSegmenter
// ============================================================================

#[test]
fn parity_abbreviation_mr() {
    // Mirror Python test: Mr. should not trigger split
    let result = segmenter().segment("Mr. Jones arrived. He sat down.");
    assert_eq!(result.len(), 2);
}

#[test]
fn parity_exclamation_with_closing_quote() {
    // Python pattern 3: !?" followed by whitespace + opening quote
    let result = segmenter().segment(r#""Attack!" "Defend the walls!""#);
    assert_eq!(result.len(), 2);
}

#[test]
fn parity_period_with_closing_quote() {
    // Python pattern 2: period + closing quote
    let result = segmenter().segment(r#""The door is locked." He pulled out a key."#);
    assert_eq!(result.len(), 2);
}
