//! Story 15-6: Intent router false positive tests for combat classification
//!
//! RED phase — the keyword classifier uses substring matching (`lower.contains(w)`)
//! which produces false positives when combat keywords appear in non-combat context:
//!   - "stricken from the will" matches "strike"
//!   - "I cast my eyes over" matches "cast"
//!   - "a charge to the heirs" matches "charge"
//!   - "hit upon an idea" matches "hit"
//!   - "throw a party" matches "throw"
//!   - "dodge the question" matches "dodge"
//!
//! These tests assert that contextual/figurative uses of combat words do NOT
//! route to creature_smith. The fix likely requires word-boundary matching or
//! a small negative-context list.

use sidequest_agents::agents::intent_router::{Intent, IntentRouter};

// ============================================================================
// AC-1 (negative): Non-combat actions must NOT classify as Combat
// ============================================================================

#[test]
fn will_reading_stricken_not_combat() {
    // "stricken from the will" — legal context, not a sword strike
    let route = IntentRouter::classify_keywords("the name was stricken from the will");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'stricken from the will' is legal language, not combat"
    );
}

#[test]
fn casting_eyes_not_combat() {
    // "cast my eyes over" — figurative, not spellcasting
    let route = IntentRouter::classify_keywords("I cast my eyes over the document");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'cast my eyes' is figurative, not a spell cast"
    );
}

#[test]
fn charge_to_heirs_not_combat() {
    // "a charge to the heirs" — legal instruction, not a physical charge
    let route = IntentRouter::classify_keywords("the will included a charge to the heirs");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'charge to the heirs' is a directive, not a combat charge"
    );
}

#[test]
fn hit_upon_idea_not_combat() {
    // "hit upon an idea" — figurative
    let route = IntentRouter::classify_keywords("I hit upon an idea about the inheritance");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'hit upon an idea' is figurative, not physical violence"
    );
}

#[test]
fn throw_a_party_not_combat() {
    // "throw a celebration" — social, not physical
    let route = IntentRouter::classify_keywords("I throw a celebration for the reading");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'throw a celebration' is social, not combat"
    );
}

#[test]
fn dodge_question_not_combat() {
    // "dodge the question" — conversational evasion
    let route = IntentRouter::classify_keywords("I dodge the question about my whereabouts");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'dodge the question' is social, not physical evasion"
    );
}

#[test]
fn grab_attention_not_combat() {
    // "grab their attention" — figurative
    let route = IntentRouter::classify_keywords("I try to grab their attention during the reading");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'grab their attention' is figurative, not a physical grab"
    );
}

#[test]
fn fire_the_servant_not_combat() {
    // "fire the butler" — employment termination, not projectile
    let route = IntentRouter::classify_keywords("I want to fire the butler after the reading");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'fire the butler' is termination, not combat"
    );
}

#[test]
fn striking_resemblance_not_combat() {
    // "a striking resemblance" — descriptive adjective
    let route = IntentRouter::classify_keywords("she bears a striking resemblance to the portrait");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'striking resemblance' is descriptive, not an attack"
    );
}

#[test]
fn block_of_text_not_combat() {
    // "block of text" — reading context
    let route = IntentRouter::classify_keywords("I read the next block of text in the will");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'block of text' is reading, not blocking an attack"
    );
}

#[test]
fn swing_of_opinion_not_combat() {
    // "swing of opinion" — social context
    let route = IntentRouter::classify_keywords("there was a swing of opinion among the heirs");
    assert_ne!(
        route.intent(),
        Intent::Combat,
        "'swing of opinion' is social, not a weapon swing"
    );
}

// ============================================================================
// Positive controls — real combat still works
// ============================================================================

#[test]
fn direct_attack_still_classified_as_combat() {
    let route = IntentRouter::classify_keywords("I attack the goblin with my sword");
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn cast_fireball_still_classified_as_combat() {
    let route = IntentRouter::classify_keywords("I cast fireball at the enemies");
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn draw_sword_still_classified_as_combat() {
    let route = IntentRouter::classify_keywords("I draw my sword and charge at the bandit");
    assert_eq!(route.intent(), Intent::Combat);
}
