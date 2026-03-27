//! Tension tracker — dual-track pacing model for combat drama.
//!
//! Tracks two independent tension axes:
//! - **action_tension** (gambler's ramp): rises during consecutive low-action
//!   turns, drops when something dramatic happens. Measures how "overdue" action is.
//! - **stakes_tension** (HP-based): rises as characters take damage or are in
//!   danger, drops as they heal/rest. Measures how much is at stake.
//!
//! The combined **drama_weight** is `max(action_tension, stakes_tension)` with
//! event spike injection and exponential decay.
//!
//! Story 5-1: TensionTracker struct — dual-track model with action tension
//! (gambler's ramp) and stakes tension (HP-based).
//!
//! Story 5-2: Combat event classification — categorize combat outcomes as
//! boring/dramatic, track boring_streak.
//!
//! Story 5-7: Pacing hints — PacingHint, DeliveryMode, DramaThresholds,
//! and TensionTracker::pacing_hint() for narrator prompt injection.

use crate::combat::RoundResult;

/// Drama-aware text delivery mode — controls how narration is revealed to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeliveryMode {
    /// Full text appears at once (low drama).
    Instant,
    /// Text reveals sentence by sentence (mid drama).
    Sentence,
    /// Text streams word by word, typewriter style (high drama).
    Streaming,
}

/// Genre-tunable breakpoints for pacing decisions.
#[derive(Debug, Clone)]
pub struct DramaThresholds {
    /// Drama weight at or above which delivery switches from Instant to Sentence.
    pub sentence_delivery_min: f64,
    /// Drama weight above which delivery switches from Sentence to Streaming.
    pub streaming_delivery_min: f64,
    /// Drama weight above which image rendering is triggered (beat filter).
    pub render_threshold: f64,
    /// Consecutive boring turns before an escalation beat hint is injected.
    pub escalation_streak: u32,
    /// Number of boring turns to reach action_tension 1.0 (gambler's ramp length).
    pub ramp_length: u32,
}

impl Default for DramaThresholds {
    fn default() -> Self {
        Self {
            sentence_delivery_min: 0.30,
            streaming_delivery_min: 0.70,
            render_threshold: 0.40,
            escalation_streak: 5,
            ramp_length: 8,
        }
    }
}

/// Pacing guidance for a single turn — computed from TensionTracker state.
#[derive(Debug, Clone)]
pub struct PacingHint {
    /// Combined drama metric from the tension tracker (0.0–1.0).
    pub drama_weight: f64,
    /// Suggested narration length in sentences (1–6).
    pub target_sentences: u8,
    /// How the client should reveal the narration text.
    pub delivery_mode: DeliveryMode,
    /// Optional escalation beat directive when boring streak exceeds threshold.
    pub escalation_beat: Option<String>,
}

impl PacingHint {
    /// Produce a narrator-facing directive string for prompt injection.
    pub fn narrator_directive(&self) -> String {
        format!(
            "Target approximately {} sentence(s) for this narration. Drama level: {:.0}%.",
            self.target_sentences,
            self.drama_weight * 100.0,
        )
    }
}

/// Combat event classification for the gambler's ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEvent {
    /// Low-action turn — ramps action tension.
    Boring,
    /// High-action moment — drops action tension and resets streak.
    Dramatic,
    /// Routine action — no effect on action tension.
    Normal,
}

/// Dual-track tension model combining action tension (gambler's ramp)
/// and stakes tension (HP-based).
#[derive(Debug, Clone, Default)]
pub struct TensionTracker {
    action_tension: f64,
    stakes_tension: f64,
    spike: f64,
    boring_streak: u32,
}

/// Base increment per boring turn, multiplied by streak count.
const BORING_BASE: f64 = 0.05;
/// Multiplicative decay factor for action tension per tick.
const ACTION_DECAY: f64 = 0.9;
/// Multiplicative decay factor for spike per tick.
const SPIKE_DECAY: f64 = 0.5;
/// Threshold below which spike snaps to zero.
const SPIKE_FLOOR: f64 = 1e-6;

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

impl TensionTracker {
    /// Create a tracker with zero tensions.
    pub fn new() -> Self {
        Self {
            action_tension: 0.0,
            stakes_tension: 0.0,
            spike: 0.0,
            boring_streak: 0,
        }
    }

    /// Create a tracker with custom initial values, clamped to 0.0–1.0.
    pub fn with_values(action: f64, stakes: f64) -> Self {
        Self {
            action_tension: clamp01(action),
            stakes_tension: clamp01(stakes),
            spike: 0.0,
            boring_streak: 0,
        }
    }

    /// Current action tension (gambler's ramp track).
    pub fn action_tension(&self) -> f64 {
        self.action_tension
    }

    /// Current stakes tension (HP-based track).
    pub fn stakes_tension(&self) -> f64 {
        self.stakes_tension
    }

    /// Combined drama metric: `max(action, stakes) + spike`, clamped to 1.0.
    pub fn drama_weight(&self) -> f64 {
        clamp01(self.action_tension.max(self.stakes_tension) + self.spike)
    }

    /// Current effective spike value after decay.
    pub fn active_spike(&self) -> f64 {
        self.spike
    }

    /// Current boring streak count — consecutive boring turns without a dramatic event.
    pub fn boring_streak(&self) -> u32 {
        self.boring_streak
    }

    /// Inject a temporary drama boost, clamped to 1.0.
    pub fn inject_spike(&mut self, amount: f64) {
        self.spike = clamp01(self.spike + amount);
    }

    /// Record a combat event, updating action tension via the gambler's ramp.
    pub fn record_event(&mut self, event: CombatEvent) {
        match event {
            CombatEvent::Boring => {
                self.boring_streak += 1;
                self.action_tension = clamp01(
                    self.action_tension + BORING_BASE * self.boring_streak as f64,
                );
            }
            CombatEvent::Dramatic => {
                self.action_tension = 0.0;
                self.boring_streak = 0;
            }
            CombatEvent::Normal => {}
        }
    }

    /// Update stakes tension from HP values. `stakes = 1.0 - (current / max)`.
    pub fn update_stakes(&mut self, current_hp: i32, max_hp: i32) {
        debug_assert!(max_hp > 0, "max_hp must be positive");
        self.stakes_tension = clamp01(1.0 - current_hp as f64 / max_hp as f64);
    }

    /// Advance one tick: decay action tension and spike. Stakes are HP-driven only.
    pub fn tick(&mut self) {
        self.action_tension *= ACTION_DECAY;
        self.spike *= SPIKE_DECAY;
        if self.spike < SPIKE_FLOOR {
            self.spike = 0.0;
        }
    }

    /// Compute a pacing hint from the current tension state and genre thresholds.
    pub fn pacing_hint(&self, thresholds: &DramaThresholds) -> PacingHint {
        let dw = self.drama_weight();

        let delivery_mode = if dw > thresholds.streaming_delivery_min {
            DeliveryMode::Streaming
        } else if dw >= thresholds.sentence_delivery_min {
            DeliveryMode::Sentence
        } else {
            DeliveryMode::Instant
        };

        // Linear interpolation: 1 + floor(drama_weight * 5), range 1–6
        let target_sentences = 1 + (dw * 5.0).floor() as u8;

        let escalation_beat = if self.boring_streak >= thresholds.escalation_streak {
            Some("The environment shifts — introduce a new element to break the monotony.".to_string())
        } else {
            None
        };

        PacingHint {
            drama_weight: dw,
            target_sentences,
            delivery_mode,
            escalation_beat,
        }
    }
}

/// Dramatic damage threshold — total round damage at or above this is dramatic.
const DRAMATIC_DAMAGE_THRESHOLD: i32 = 15;

/// Classify a combat round result as Boring, Dramatic, or Normal.
///
/// Classification rules:
/// - **Dramatic:** a combatant was killed (`killed` names the deceased), total
///   damage >= threshold, or new status effects were applied.
/// - **Boring:** zero effective damage and no new effects.
/// - **Normal:** some damage dealt but below the dramatic threshold, no kills or effects.
pub fn classify_round(round: &RoundResult, killed: Option<&str>) -> CombatEvent {
    // A kill is always dramatic
    if killed.is_some() {
        return CombatEvent::Dramatic;
    }

    // New status effects are dramatic
    if !round.effects_applied.is_empty() {
        return CombatEvent::Dramatic;
    }

    let total_damage: i32 = round
        .damage_events
        .iter()
        .map(|e| e.damage.max(0))
        .sum();

    if total_damage >= DRAMATIC_DAMAGE_THRESHOLD {
        return CombatEvent::Dramatic;
    }

    if total_damage == 0 {
        return CombatEvent::Boring;
    }

    CombatEvent::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Construction
    // =========================================================================

    #[test]
    fn new_tracker_has_zero_tensions() {
        let tracker = TensionTracker::new();
        assert_eq!(tracker.action_tension(), 0.0);
        assert_eq!(tracker.stakes_tension(), 0.0);
        assert_eq!(tracker.drama_weight(), 0.0);
    }

    #[test]
    fn new_with_initial_values() {
        let tracker = TensionTracker::with_values(0.3, 0.5);
        assert_eq!(tracker.action_tension(), 0.3);
        assert_eq!(tracker.stakes_tension(), 0.5);
    }

    #[test]
    fn initial_values_clamped_to_unit_range() {
        let tracker = TensionTracker::with_values(1.5, -0.2);
        assert_eq!(tracker.action_tension(), 1.0);
        assert_eq!(tracker.stakes_tension(), 0.0);
    }

    // =========================================================================
    // Action tension — gambler's ramp
    // =========================================================================

    #[test]
    fn boring_turn_increases_action_tension() {
        let mut tracker = TensionTracker::new();
        tracker.record_event(CombatEvent::Boring);
        assert!(tracker.action_tension() > 0.0, "boring turn should ramp action tension");
    }

    #[test]
    fn consecutive_boring_turns_ramp_faster() {
        let mut tracker = TensionTracker::new();
        tracker.record_event(CombatEvent::Boring);
        let after_one = tracker.action_tension();
        tracker.record_event(CombatEvent::Boring);
        let after_two = tracker.action_tension();
        // Gambler's ramp: each boring turn adds more than the last
        assert!(
            after_two - after_one > after_one,
            "gambler's ramp should accelerate: first bump {after_one}, second bump {}",
            after_two - after_one,
        );
    }

    #[test]
    fn dramatic_event_drops_action_tension() {
        let mut tracker = TensionTracker::new();
        // Build up some tension
        for _ in 0..5 {
            tracker.record_event(CombatEvent::Boring);
        }
        let before = tracker.action_tension();
        tracker.record_event(CombatEvent::Dramatic);
        assert!(
            tracker.action_tension() < before,
            "dramatic event should drop action tension",
        );
    }

    #[test]
    fn dramatic_event_resets_boring_streak() {
        let mut tracker = TensionTracker::new();
        for _ in 0..3 {
            tracker.record_event(CombatEvent::Boring);
        }
        tracker.record_event(CombatEvent::Dramatic);
        // After reset, next boring turn should ramp from scratch
        tracker.record_event(CombatEvent::Boring);
        let fresh_boring = tracker.action_tension();
        // Compare to a tracker that only had one boring turn from zero
        let mut fresh = TensionTracker::new();
        fresh.record_event(CombatEvent::Boring);
        assert_eq!(
            fresh_boring, fresh.action_tension(),
            "after dramatic reset, boring ramp should restart from scratch",
        );
    }

    // =========================================================================
    // Stakes tension — HP-based
    // =========================================================================

    #[test]
    fn damage_raises_stakes_tension() {
        let mut tracker = TensionTracker::new();
        // Character at 80/100 HP → took 20% damage
        tracker.update_stakes(80, 100);
        assert!(tracker.stakes_tension() > 0.0, "damage should raise stakes tension");
    }

    #[test]
    fn low_hp_means_high_stakes() {
        let mut tracker = TensionTracker::new();
        tracker.update_stakes(10, 100);
        assert!(
            tracker.stakes_tension() > 0.8,
            "10% HP should yield very high stakes tension, got {}",
            tracker.stakes_tension(),
        );
    }

    #[test]
    fn full_hp_means_zero_stakes() {
        let mut tracker = TensionTracker::new();
        tracker.update_stakes(100, 100);
        assert_eq!(
            tracker.stakes_tension(),
            0.0,
            "full HP should mean zero stakes tension",
        );
    }

    #[test]
    fn healing_reduces_stakes_tension() {
        let mut tracker = TensionTracker::new();
        tracker.update_stakes(30, 100); // 70% damage
        let before = tracker.stakes_tension();
        tracker.update_stakes(60, 100); // healed to 60%
        assert!(
            tracker.stakes_tension() < before,
            "healing should reduce stakes tension",
        );
    }

    #[test]
    fn zero_hp_maxes_stakes() {
        let mut tracker = TensionTracker::new();
        tracker.update_stakes(0, 100);
        assert_eq!(
            tracker.stakes_tension(),
            1.0,
            "0 HP should be max stakes",
        );
    }

    // =========================================================================
    // Drama weight — combined metric
    // =========================================================================

    #[test]
    fn drama_weight_is_max_of_both_tracks() {
        let mut tracker = TensionTracker::with_values(0.3, 0.7);
        assert_eq!(
            tracker.drama_weight(),
            0.7,
            "drama_weight should be max(action, stakes)",
        );
    }

    #[test]
    fn drama_weight_follows_action_when_higher() {
        let mut tracker = TensionTracker::with_values(0.9, 0.2);
        assert_eq!(
            tracker.drama_weight(),
            0.9,
            "drama_weight should follow action_tension when it's higher",
        );
    }

    #[test]
    fn drama_weight_includes_spike() {
        let mut tracker = TensionTracker::new();
        tracker.inject_spike(0.8);
        assert!(
            tracker.drama_weight() >= 0.8,
            "spike should boost drama_weight, got {}",
            tracker.drama_weight(),
        );
    }

    // =========================================================================
    // Spike injection and decay
    // =========================================================================

    #[test]
    fn spike_injection_adds_temporary_boost() {
        let mut tracker = TensionTracker::new();
        tracker.inject_spike(0.6);
        assert!(
            tracker.active_spike() > 0.0,
            "injected spike should be visible",
        );
    }

    #[test]
    fn spike_decays_over_ticks() {
        let mut tracker = TensionTracker::new();
        tracker.inject_spike(0.8);
        let initial_spike = tracker.active_spike();
        tracker.tick();
        assert!(
            tracker.active_spike() < initial_spike,
            "spike should decay after tick",
        );
    }

    #[test]
    fn spike_fully_decays_to_zero() {
        let mut tracker = TensionTracker::new();
        tracker.inject_spike(0.5);
        // Tick enough times for full decay
        for _ in 0..20 {
            tracker.tick();
        }
        assert!(
            tracker.active_spike() < f64::EPSILON,
            "spike should fully decay to ~0, got {}",
            tracker.active_spike(),
        );
    }

    #[test]
    fn spike_clamped_to_unit_range() {
        let mut tracker = TensionTracker::new();
        tracker.inject_spike(1.5);
        assert!(
            tracker.active_spike() <= 1.0,
            "spike should be clamped to 1.0",
        );
    }

    // =========================================================================
    // Tension decay over quiet turns
    // =========================================================================

    #[test]
    fn action_tension_decays_on_tick() {
        let mut tracker = TensionTracker::with_values(0.8, 0.0);
        tracker.tick();
        assert!(
            tracker.action_tension() < 0.8,
            "action tension should decay on tick without events",
        );
    }

    #[test]
    fn stakes_tension_does_not_decay_on_tick() {
        // Stakes are HP-based, they only change when HP changes
        let mut tracker = TensionTracker::with_values(0.0, 0.6);
        tracker.tick();
        assert_eq!(
            tracker.stakes_tension(),
            0.6,
            "stakes tension should NOT decay on tick (it's HP-driven)",
        );
    }

    #[test]
    fn multiple_ticks_decay_toward_zero() {
        let mut tracker = TensionTracker::with_values(0.9, 0.0);
        for _ in 0..50 {
            tracker.tick();
        }
        assert!(
            tracker.action_tension() < 0.01,
            "many quiet ticks should decay action tension near zero, got {}",
            tracker.action_tension(),
        );
    }

    // =========================================================================
    // Edge cases and clamping
    // =========================================================================

    #[test]
    fn action_tension_never_exceeds_one() {
        let mut tracker = TensionTracker::new();
        for _ in 0..100 {
            tracker.record_event(CombatEvent::Boring);
        }
        assert!(
            tracker.action_tension() <= 1.0,
            "action tension must stay <= 1.0, got {}",
            tracker.action_tension(),
        );
    }

    #[test]
    fn action_tension_never_goes_negative() {
        let mut tracker = TensionTracker::new();
        // Dramatic event on an already-zero tracker
        tracker.record_event(CombatEvent::Dramatic);
        assert!(
            tracker.action_tension() >= 0.0,
            "action tension must stay >= 0.0",
        );
    }

    #[test]
    fn drama_weight_clamped_with_spike() {
        let mut tracker = TensionTracker::with_values(0.9, 0.9);
        tracker.inject_spike(0.8);
        assert!(
            tracker.drama_weight() <= 1.0,
            "drama_weight should not exceed 1.0 even with spike, got {}",
            tracker.drama_weight(),
        );
    }

    #[test]
    fn both_tensions_at_max() {
        let tracker = TensionTracker::with_values(1.0, 1.0);
        assert_eq!(tracker.drama_weight(), 1.0);
    }

    #[test]
    fn both_tensions_at_zero() {
        let tracker = TensionTracker::with_values(0.0, 0.0);
        assert_eq!(tracker.drama_weight(), 0.0);
    }

    // =========================================================================
    // CombatEvent classification
    // =========================================================================

    #[test]
    fn combat_event_variants_exist() {
        // Ensure the enum has the expected variants
        let _boring = CombatEvent::Boring;
        let _dramatic = CombatEvent::Dramatic;
        let _normal = CombatEvent::Normal;
    }

    #[test]
    fn normal_event_has_mild_effect_on_action_tension() {
        let mut tracker = TensionTracker::new();
        // Build up some action tension first
        for _ in 0..3 {
            tracker.record_event(CombatEvent::Boring);
        }
        let before = tracker.action_tension();
        tracker.record_event(CombatEvent::Normal);
        // Normal events should slightly reduce or maintain (not ramp like boring)
        assert!(
            tracker.action_tension() <= before,
            "normal event should not increase action tension",
        );
    }
}
