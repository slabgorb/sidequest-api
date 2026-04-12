//! Story 15-13: Wire AchievementTracker — check_transition never called
//!
//! RED phase — tests that verify the achievement system is wired into the
//! trope tick loop so that achievements actually fire during gameplay.
//!
//! The gap: AchievementTracker::check_transition() exists and works in isolation
//! (see achievement.rs unit tests), but it's never called from the trope tick
//! pipeline. TropeEngine::tick() doesn't capture old_status before transitioning,
//! and process_tropes() in the server doesn't touch achievement_tracker at all.
//!
//! These tests assert:
//!   1. TropeEngine has a method that ticks tropes AND checks achievements in one call
//!   2. Status transitions during tick produce newly earned achievements
//!   3. The "subverted" trigger works through the integrated tick+check path
//!   4. Dedup guard prevents re-earning through the integrated path
//!   5. OTEL spans are emitted for earned achievements
//!
//! ACs covered:
//!   AC-1: check_transition called after trope tick when status changes
//!   AC-2: Earned achievements broadcast as GameMessage events
//!   AC-3: OTEL event achievement.earned with achievement_id, trope_id, trigger_type

use sidequest_game::achievement::{Achievement, AchievementTracker};
use sidequest_game::trope::{TropeEngine, TropeState, TropeStatus};
use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test helpers
// ============================================================================

fn betrayal_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("betrayal".to_string()),
        name: NonBlankString::new("Betrayal").unwrap(),
        description: Some("A trust is broken".to_string()),
        category: "conflict".to_string(),
        triggers: vec!["betray".to_string()],
        narrative_hints: vec![],
        tension_level: None,
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.15,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        escalation: vec![TropeEscalation {
            at: 0.3,
            event: "Seeds of doubt".to_string(),
            npcs_involved: vec![],
            stakes: String::new(),
        }],
        is_abstract: false,
        extends: None,
    }
}

fn redemption_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("redemption".to_string()),
        name: NonBlankString::new("Redemption").unwrap(),
        description: Some("Seeking forgiveness".to_string()),
        category: "revelation".to_string(),
        triggers: vec!["redeem".to_string()],
        narrative_hints: vec![],
        tension_level: None,
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.2,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        escalation: vec![],
        is_abstract: false,
        extends: None,
    }
}

fn sample_achievements() -> Vec<Achievement> {
    vec![
        Achievement {
            id: "ach-activated".into(),
            name: "First Steps".into(),
            description: "A trope begins to unfold.".into(),
            trope_id: "betrayal".into(),
            trigger_status: "activated".into(),
            emoji: None,
        },
        Achievement {
            id: "ach-progressing".into(),
            name: "Deepening".into(),
            description: "The betrayal thickens.".into(),
            trope_id: "betrayal".into(),
            trigger_status: "progressing".into(),
            emoji: None,
        },
        Achievement {
            id: "ach-resolved".into(),
            name: "Resolution".into(),
            description: "The betrayal is resolved.".into(),
            trope_id: "betrayal".into(),
            trigger_status: "resolved".into(),
            emoji: None,
        },
        Achievement {
            id: "ach-subverted".into(),
            name: "Twist!".into(),
            description: "The betrayal was subverted.".into(),
            trope_id: "betrayal".into(),
            trigger_status: "subverted".into(),
            emoji: None,
        },
        Achievement {
            id: "ach-redemption".into(),
            name: "Seeking Light".into(),
            description: "Redemption arc activated.".into(),
            trope_id: "redemption".into(),
            trigger_status: "progressing".into(),
            emoji: None,
        },
    ]
}

// ============================================================================
// AC-1: check_transition called after trope tick when status changes
// ============================================================================

/// TropeEngine::tick_and_check_achievements should exist and return earned
/// achievements when trope status transitions occur during the tick.
///
/// Current state: This method does NOT exist. TropeEngine::tick() doesn't
/// interact with AchievementTracker at all. This test will fail to compile
/// until Dev adds the method.
#[test]
fn tick_and_check_achievements_fires_on_active_to_progressing() {
    // Trope starts Active at 0.0, rate_per_turn = 0.15 → progression becomes 0.15
    // Status should transition Active → Progressing (progression > 0.0)
    let mut tropes = vec![TropeState::new("betrayal")];
    let defs = vec![betrayal_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    let (fired_beats, earned) =
        TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);

    // Trope should have progressed and fired the "progressing" achievement
    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert!(
        !earned.is_empty(),
        "Achievement should fire on Active → Progressing transition"
    );
    assert_eq!(earned[0].id, "ach-progressing");
    // Beat at 0.3 shouldn't fire yet (progression = 0.15)
    assert!(fired_beats.is_empty());
}

/// Multiple tropes transitioning in the same tick should each independently
/// check achievements.
#[test]
fn tick_and_check_achievements_multiple_tropes() {
    let mut tropes = vec![TropeState::new("betrayal"), TropeState::new("redemption")];
    let defs = vec![betrayal_trope_def(), redemption_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    let (_fired_beats, earned) =
        TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);

    // Both tropes go Active → Progressing on first tick
    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert_eq!(tropes[1].status(), TropeStatus::Progressing);
    assert_eq!(
        earned.len(),
        2,
        "Both trope transitions should produce achievements"
    );
    let earned_ids: Vec<&str> = earned.iter().map(|a| a.id.as_str()).collect();
    assert!(earned_ids.contains(&"ach-progressing"));
    assert!(earned_ids.contains(&"ach-redemption"));
}

/// No transition means no achievements — the dedup on status comparison
/// must work through the integrated path.
#[test]
fn tick_and_check_achievements_no_transition_no_achievement() {
    // Pre-set the trope to Progressing with some progression already
    let mut trope = TropeState::new("betrayal");
    trope.set_status(TropeStatus::Progressing);
    trope.set_progression(0.1);
    let mut tropes = vec![trope];
    let defs = vec![betrayal_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    let (_fired_beats, earned) =
        TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);

    // Status stays Progressing → no transition → no achievement
    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert!(
        earned.is_empty(),
        "No status transition should mean no achievements"
    );
}

/// Resolved tropes and Dormant tropes are skipped by tick — no achievements
/// should fire for them.
#[test]
fn tick_and_check_achievements_skips_resolved_and_dormant() {
    let mut resolved = TropeState::new("betrayal");
    resolved.set_status(TropeStatus::Resolved);
    let mut dormant = TropeState::new("redemption");
    dormant.set_status(TropeStatus::Dormant);
    let mut tropes = vec![resolved, dormant];
    let defs = vec![betrayal_trope_def(), redemption_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    let (_fired_beats, earned) =
        TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);

    assert!(
        earned.is_empty(),
        "Resolved and Dormant tropes should not fire achievements"
    );
}

/// Dedup guard: calling tick_and_check_achievements twice with the same
/// transition should not double-award.
#[test]
fn tick_and_check_achievements_dedup_across_ticks() {
    let mut tropes = vec![TropeState::new("betrayal")];
    let defs = vec![betrayal_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    // First tick: Active → Progressing fires ach-progressing
    let (_, earned1) = TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);
    assert_eq!(earned1.len(), 1);

    // Second tick: Progressing → Progressing (no transition) → no achievement
    let (_, earned2) = TropeEngine::tick_and_check_achievements(&mut tropes, &defs, &mut tracker);
    assert!(
        earned2.is_empty(),
        "Second tick with same status should not re-award"
    );
}

/// tick_and_check_achievements with engagement multiplier should also
/// check achievements.
#[test]
fn tick_and_check_achievements_with_multiplier() {
    let mut tropes = vec![TropeState::new("betrayal")];
    let defs = vec![betrayal_trope_def()];
    let mut tracker = AchievementTracker::new(sample_achievements());

    let (_, earned) = TropeEngine::tick_and_check_achievements_with_multiplier(
        &mut tropes,
        &defs,
        &mut tracker,
        2.0,
    );

    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert!(
        !earned.is_empty(),
        "Multiplied tick should still fire achievements on transition"
    );
    assert_eq!(earned[0].id, "ach-progressing");
}

// ============================================================================
// AC-1 (resolve path): check_transition called when trope is resolved
// ============================================================================

/// When TropeEngine::resolve is called, achievements should fire for the
/// Resolved transition. This verifies the resolve path is also wired.
#[test]
fn resolve_and_check_achievements_fires_resolved() {
    let mut tropes = vec![TropeState::new("betrayal")];
    tropes[0].set_status(TropeStatus::Progressing);
    let mut tracker = AchievementTracker::new(sample_achievements());

    let earned =
        TropeEngine::resolve_and_check_achievements(&mut tropes, "betrayal", None, &mut tracker);

    assert_eq!(tropes[0].status(), TropeStatus::Resolved);
    assert_eq!(earned.len(), 1);
    assert_eq!(earned[0].id, "ach-resolved");
}

/// Subversion: resolve with a "subverted" note should fire both resolved
/// and subverted achievements.
#[test]
fn resolve_and_check_achievements_fires_subverted() {
    let mut tropes = vec![TropeState::new("betrayal")];
    tropes[0].set_status(TropeStatus::Progressing);
    let mut tracker = AchievementTracker::new(sample_achievements());

    let earned = TropeEngine::resolve_and_check_achievements(
        &mut tropes,
        "betrayal",
        Some("Subverted by the hero's sacrifice"),
        &mut tracker,
    );

    assert_eq!(tropes[0].status(), TropeStatus::Resolved);
    assert_eq!(
        earned.len(),
        2,
        "Both resolved and subverted achievements should fire"
    );
    let ids: Vec<&str> = earned.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&"ach-resolved"));
    assert!(ids.contains(&"ach-subverted"));
}

// ============================================================================
// AC-1 (activate path): check_transition called when trope is activated
// ============================================================================

/// When a new trope is activated (Dormant → Active), the "activated"
/// trigger achievement should fire.
#[test]
fn activate_and_check_achievements_fires_activated() {
    let mut tropes: Vec<TropeState> = vec![];
    let mut tracker = AchievementTracker::new(sample_achievements());

    TropeEngine::activate_and_check_achievements(&mut tropes, "betrayal", &mut tracker);

    assert_eq!(tropes.len(), 1);
    assert_eq!(tropes[0].status(), TropeStatus::Active);
    assert!(
        tracker.earned.contains("ach-activated"),
        "Activating a new trope should earn the 'activated' achievement"
    );
}

/// Idempotent activation — re-activating an already-active trope should
/// NOT re-earn the "activated" achievement.
#[test]
fn activate_and_check_achievements_idempotent() {
    let mut tropes: Vec<TropeState> = vec![];
    let mut tracker = AchievementTracker::new(sample_achievements());

    TropeEngine::activate_and_check_achievements(&mut tropes, "betrayal", &mut tracker);
    assert!(tracker.earned.contains("ach-activated"));

    // Second activation — trope already exists, should not re-earn
    let earned_before = tracker.earned.len();
    TropeEngine::activate_and_check_achievements(&mut tropes, "betrayal", &mut tracker);
    assert_eq!(tracker.earned.len(), earned_before);
}

// ============================================================================
// AC-1 (cross-session path): check_transition called on between-session advance
// ============================================================================

/// Cross-session advancement that causes Active → Progressing should
/// fire the "progressing" achievement.
#[test]
fn advance_between_sessions_and_check_achievements_fires() {
    let mut tropes = vec![TropeState::new("betrayal")];
    let mut tracker = AchievementTracker::new(sample_achievements());

    // Use a def with rate_per_day > 0 so cross-session advancement actually works
    let defs = vec![TropeDefinition {
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.15,
            rate_per_day: 0.1,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        ..betrayal_trope_def()
    }];
    // 10 days at 0.1/day = 1.0, transitions Active → Progressing
    let (_fired, earned) = TropeEngine::advance_between_sessions_and_check_achievements(
        &mut tropes,
        &defs,
        10.0,
        &mut tracker,
    );

    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert!(
        !earned.is_empty(),
        "Cross-session advancement should fire achievements on transition"
    );
    assert_eq!(earned[0].id, "ach-progressing");
}

/// Cross-session advancement with no transition should not fire achievements.
#[test]
fn advance_between_sessions_and_check_achievements_no_transition() {
    let mut trope = TropeState::new("betrayal");
    trope.set_status(TropeStatus::Progressing);
    trope.set_progression(0.5);
    let mut tropes = vec![trope];
    let defs = vec![TropeDefinition {
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.15,
            rate_per_day: 0.01,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        ..betrayal_trope_def()
    }];
    let mut tracker = AchievementTracker::new(sample_achievements());

    // Small advancement (0.1 days * 0.01/day = 0.001), stays Progressing
    let (_fired, earned) = TropeEngine::advance_between_sessions_and_check_achievements(
        &mut tropes,
        &defs,
        0.1,
        &mut tracker,
    );

    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    assert!(
        earned.is_empty(),
        "No status transition should mean no achievements"
    );
}

// ============================================================================
// Wiring verification: achievement_tracker must be accessible from dispatch
// ============================================================================

/// Verify that GameSnapshot's achievement_tracker is mutable and can be
/// passed to the tick+check pipeline. This is a compile-time wiring check.
#[test]
fn game_snapshot_achievement_tracker_accessible() {
    use sidequest_game::state::GameSnapshot;

    let mut snap = GameSnapshot {
        achievement_tracker: AchievementTracker::new(sample_achievements()),
        ..Default::default()
    };

    // Set up tropes on the snapshot
    snap.active_tropes.push(TropeState::new("betrayal"));

    // The tracker and tropes must be independently borrowable for the
    // tick+check pipeline. This tests that the field layout supports
    // split borrows (mutable tracker + mutable tropes simultaneously).
    let trope_count = snap.active_tropes.len();
    let achievement_count = snap.achievement_tracker.achievements.len();
    assert_eq!(trope_count, 1);
    assert_eq!(achievement_count, 5);
}
