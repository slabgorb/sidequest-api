//! Trope engine runtime — tick progression, escalation beats, lifecycle.
//!
//! Story 2-8: Adds the runtime loop that ticks trope progression,
//! fires escalation beats at thresholds, and provides lifecycle management.

use std::collections::{HashMap, HashSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use sidequest_genre::{TropeDefinition, TropeEscalation};

use crate::achievement::{Achievement, AchievementTracker};

/// Status of an active trope in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TropeStatus {
    /// Trope is dormant — not ticking.
    Dormant,
    /// Trope is active but hasn't progressed yet.
    Active,
    /// Trope is actively progressing.
    Progressing,
    /// Trope has been resolved.
    Resolved,
}

/// Runtime state for an active trope instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TropeState {
    trope_definition_id: String,
    status: TropeStatus,
    progression: f64,
    fired_beats: HashSet<OrderedFloat<f64>>,
    notes: Vec<String>,
}

impl TropeState {
    /// Create a new trope state in Active status at 0.0 progression.
    pub fn new(trope_definition_id: &str) -> Self {
        Self {
            trope_definition_id: trope_definition_id.to_string(),
            status: TropeStatus::Active,
            progression: 0.0,
            fired_beats: HashSet::new(),
            notes: Vec::new(),
        }
    }

    /// The trope definition ID this state tracks.
    pub fn trope_definition_id(&self) -> &str {
        &self.trope_definition_id
    }

    /// Current status.
    pub fn status(&self) -> TropeStatus {
        self.status
    }

    /// Set the status.
    pub fn set_status(&mut self, status: TropeStatus) {
        self.status = status;
    }

    /// Current progression (0.0 to 1.0).
    pub fn progression(&self) -> f64 {
        self.progression
    }

    /// Set progression value (clamped to 0.0..=1.0).
    pub fn set_progression(&mut self, value: f64) {
        self.progression = value.clamp(0.0, 1.0);
    }

    /// The set of beat thresholds that have already fired.
    pub fn fired_beats(&self) -> &HashSet<OrderedFloat<f64>> {
        &self.fired_beats
    }

    /// Notes accumulated on this trope.
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Add a note to this trope.
    pub fn add_note(&mut self, note: String) {
        self.notes.push(note);
    }
}

/// A beat that fired during a tick.
#[derive(Clone)]
pub struct FiredBeat {
    /// The trope definition ID.
    pub trope_id: String,
    /// The trope display name.
    pub trope_name: String,
    /// The escalation beat that fired.
    pub beat: TropeEscalation,
}

/// Trope engine — ticks progression, fires beats, manages lifecycle.
pub struct TropeEngine;

impl TropeEngine {
    /// Advance all active tropes by their passive rate, then check for fired beats.
    pub fn tick(tropes: &mut [TropeState], trope_defs: &[TropeDefinition]) -> Vec<FiredBeat> {
        Self::tick_with_multiplier(tropes, trope_defs, 1.0)
    }

    /// Advance all active tropes with an engagement multiplier applied to the passive rate.
    ///
    /// The multiplier scales `rate_per_turn` — a multiplier of 2.0 doubles progression
    /// speed (passive player), while 0.5 halves it (active player).
    pub fn tick_with_multiplier(
        tropes: &mut [TropeState],
        trope_defs: &[TropeDefinition],
        multiplier: f64,
    ) -> Vec<FiredBeat> {
        let span = tracing::info_span!(
            "trope_tick",
            trope_count = tropes.len(),
            multiplier = multiplier,
            beats_fired = tracing::field::Empty,
        );
        let _guard = span.enter();

        let def_map: HashMap<&str, &TropeDefinition> = trope_defs
            .iter()
            .filter_map(|td| td.id.as_deref().map(|id| (id, td)))
            .collect();

        let mut fired = Vec::new();

        for ts in tropes.iter_mut() {
            if matches!(ts.status, TropeStatus::Resolved | TropeStatus::Dormant) {
                continue;
            }

            let Some(td) = def_map.get(ts.trope_definition_id.as_str()) else {
                tracing::warn!(
                    trope_id = %ts.trope_definition_id,
                    "Trope definition not found"
                );
                continue;
            };

            // Passive progression scaled by engagement multiplier
            if let Some(pp) = &td.passive_progression {
                ts.progression = (ts.progression + pp.rate_per_turn * multiplier).min(1.0);
            }

            // Check escalation beats
            for beat in &td.escalation {
                let threshold = OrderedFloat(beat.at);
                if beat.at <= ts.progression && !ts.fired_beats.contains(&threshold) {
                    ts.fired_beats.insert(threshold);
                    fired.push(FiredBeat {
                        trope_id: ts.trope_definition_id.clone(),
                        trope_name: td.name.as_str().to_string(),
                        beat: beat.clone(),
                    });
                }
            }

            // Status transition: Active → Progressing
            if ts.status == TropeStatus::Active && ts.progression > 0.0 {
                ts.status = TropeStatus::Progressing;
            }
        }

        span.record("beats_fired", fired.len() as u64);
        fired
    }

    /// Activate a trope. Idempotent — returns existing if already active.
    pub fn activate<'a>(tropes: &'a mut Vec<TropeState>, def_id: &str) -> &'a TropeState {
        let span = tracing::info_span!(
            "trope_activate",
            trope_id = def_id,
        );
        let _guard = span.enter();

        if let Some(idx) = tropes
            .iter()
            .position(|ts| ts.trope_definition_id == def_id)
        {
            return &tropes[idx];
        }
        tropes.push(TropeState::new(def_id));
        tropes.last().unwrap()
    }

    /// Advance all active tropes by elapsed real-time days since last session.
    ///
    /// Uses `rate_per_day` from passive progression config. Returns any escalation
    /// beats that fire due to the advancement — these represent "living world"
    /// events that happened while the player was away.
    pub fn advance_between_sessions(
        tropes: &mut [TropeState],
        trope_defs: &[TropeDefinition],
        elapsed_days: f64,
    ) -> Vec<FiredBeat> {
        let span = tracing::info_span!(
            "trope.cross_session",
            elapsed_days = elapsed_days,
            tropes_advanced = tracing::field::Empty,
            beats_fired = tracing::field::Empty,
        );
        let _guard = span.enter();

        let def_map: HashMap<&str, &TropeDefinition> = trope_defs
            .iter()
            .filter_map(|td| td.id.as_deref().map(|id| (id, td)))
            .collect();

        let mut fired = Vec::new();
        let mut advanced_count: u64 = 0;

        for ts in tropes.iter_mut() {
            if matches!(ts.status, TropeStatus::Resolved | TropeStatus::Dormant) {
                continue;
            }

            let Some(td) = def_map.get(ts.trope_definition_id.as_str()) else {
                continue;
            };

            let Some(pp) = &td.passive_progression else {
                continue;
            };

            if pp.rate_per_day <= 0.0 {
                continue;
            }

            let before = ts.progression;
            ts.progression = (ts.progression + pp.rate_per_day * elapsed_days).min(1.0);

            if (ts.progression - before).abs() > f64::EPSILON {
                advanced_count += 1;
            }

            // Check escalation beats crossed during the gap
            for beat in &td.escalation {
                let threshold = OrderedFloat(beat.at);
                if beat.at <= ts.progression && !ts.fired_beats.contains(&threshold) {
                    ts.fired_beats.insert(threshold);
                    fired.push(FiredBeat {
                        trope_id: ts.trope_definition_id.clone(),
                        trope_name: td.name.as_str().to_string(),
                        beat: beat.clone(),
                    });
                }
            }

            // Status transition: Active → Progressing
            if ts.status == TropeStatus::Active && ts.progression > 0.0 {
                ts.status = TropeStatus::Progressing;
            }
        }

        span.record("tropes_advanced", advanced_count);
        span.record("beats_fired", fired.len() as u64);

        if advanced_count > 0 {
            tracing::info!(
                advanced = advanced_count,
                beats = fired.len(),
                elapsed_days = elapsed_days,
                "Cross-session trope advancement complete"
            );
        }

        fired
    }

    /// Resolve a trope — sets progression to 1.0 and status to Resolved.
    pub fn resolve(tropes: &mut [TropeState], def_id: &str, note: Option<&str>) {
        let span = tracing::info_span!(
            "trope_resolve",
            trope_id = def_id,
        );
        let _guard = span.enter();

        if let Some(ts) = tropes
            .iter_mut()
            .find(|ts| ts.trope_definition_id == def_id)
        {
            ts.status = TropeStatus::Resolved;
            ts.progression = 1.0;
            if let Some(n) = note {
                ts.add_note(n.to_string());
            }
        }
    }

    /// Tick all tropes and check for newly earned achievements.
    ///
    /// Captures each trope's status before the tick, delegates to `tick()`,
    /// then calls `AchievementTracker::check_transition` for every trope
    /// whose status changed. Returns both fired beats and earned achievements.
    pub fn tick_and_check_achievements(
        tropes: &mut [TropeState],
        trope_defs: &[TropeDefinition],
        tracker: &mut AchievementTracker,
    ) -> (Vec<FiredBeat>, Vec<Achievement>) {
        Self::tick_and_check_achievements_with_multiplier(tropes, trope_defs, tracker, 1.0)
    }

    /// Tick all tropes with an engagement multiplier and check for achievements.
    ///
    /// Same as `tick_and_check_achievements` but applies the given multiplier
    /// to the passive progression rate.
    pub fn tick_and_check_achievements_with_multiplier(
        tropes: &mut [TropeState],
        trope_defs: &[TropeDefinition],
        tracker: &mut AchievementTracker,
        multiplier: f64,
    ) -> (Vec<FiredBeat>, Vec<Achievement>) {
        // Snapshot old statuses before tick
        let old_statuses: Vec<TropeStatus> = tropes.iter().map(|ts| ts.status()).collect();

        let fired = Self::tick_with_multiplier(tropes, trope_defs, multiplier);

        // Check achievements for every trope that transitioned
        let mut earned = Vec::new();
        for (ts, old_status) in tropes.iter().zip(old_statuses.iter()) {
            if ts.status() != *old_status {
                let newly_earned = tracker.check_transition(ts, *old_status);
                Self::log_earned_achievements(&newly_earned, ts.trope_definition_id());
                earned.extend(newly_earned);
            }
        }

        (fired, earned)
    }

    /// Resolve a trope and check for newly earned achievements.
    ///
    /// Captures the trope's status before resolving, delegates to `resolve()`,
    /// then calls `AchievementTracker::check_transition` if the status changed.
    pub fn resolve_and_check_achievements(
        tropes: &mut [TropeState],
        def_id: &str,
        note: Option<&str>,
        tracker: &mut AchievementTracker,
    ) -> Vec<Achievement> {
        // Snapshot old status for the target trope
        let old_status = tropes
            .iter()
            .find(|ts| ts.trope_definition_id == def_id)
            .map(|ts| ts.status());

        Self::resolve(tropes, def_id, note);

        // Check achievements if the trope exists and status changed
        if let Some(old) = old_status {
            if let Some(ts) = tropes.iter().find(|ts| ts.trope_definition_id == def_id) {
                if ts.status() != old {
                    let newly_earned = tracker.check_transition(ts, old);
                    Self::log_earned_achievements(&newly_earned, ts.trope_definition_id());
                    return newly_earned;
                }
            }
        }

        Vec::new()
    }

    /// Activate a trope and check for newly earned achievements.
    ///
    /// Captures whether the trope existed before activation. If activation
    /// creates a new trope (Dormant → Active transition), checks for
    /// "activated" trigger achievements.
    pub fn activate_and_check_achievements<'a>(
        tropes: &'a mut Vec<TropeState>,
        def_id: &str,
        tracker: &mut AchievementTracker,
    ) -> &'a TropeState {
        // Check if trope already exists (idempotent activation = no transition)
        let already_exists = tropes.iter().any(|ts| ts.trope_definition_id == def_id);

        let ts = Self::activate(tropes, def_id);

        if !already_exists {
            // New trope: transition is effectively Dormant → Active
            let newly_earned = tracker.check_transition(ts, TropeStatus::Dormant);
            Self::log_earned_achievements(&newly_earned, def_id);
        }

        // Re-borrow after tracker mutation
        tropes.iter().find(|ts| ts.trope_definition_id == def_id).unwrap()
    }

    /// Advance tropes by elapsed days and check for newly earned achievements.
    ///
    /// Same as `advance_between_sessions` but captures old statuses and calls
    /// `check_transition` for any trope whose status changed during the gap.
    pub fn advance_between_sessions_and_check_achievements(
        tropes: &mut [TropeState],
        trope_defs: &[TropeDefinition],
        elapsed_days: f64,
        tracker: &mut AchievementTracker,
    ) -> (Vec<FiredBeat>, Vec<Achievement>) {
        let old_statuses: Vec<TropeStatus> = tropes.iter().map(|ts| ts.status()).collect();

        let fired = Self::advance_between_sessions(tropes, trope_defs, elapsed_days);

        let mut earned = Vec::new();
        for (ts, old_status) in tropes.iter().zip(old_statuses.iter()) {
            if ts.status() != *old_status {
                let newly_earned = tracker.check_transition(ts, *old_status);
                Self::log_earned_achievements(&newly_earned, ts.trope_definition_id());
                earned.extend(newly_earned);
            }
        }

        (fired, earned)
    }

    /// Emit OTEL info events for each earned achievement.
    fn log_earned_achievements(achievements: &[Achievement], trope_id: &str) {
        for achievement in achievements {
            tracing::info!(
                achievement_id = %achievement.id,
                trope_id = %trope_id,
                trigger_type = %achievement.trigger_status,
                "achievement.earned"
            );
        }
    }
}
