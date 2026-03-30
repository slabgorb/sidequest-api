//! Combat state — round tracking, damage log, status effects.
//!
//! Decomposes the Python GameState's combat handling into a focused type
//! that owns its own mutations (port lesson #4).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::combatant::Combatant;
use crate::progression::level_to_damage;

/// Tracks the state of an active combat encounter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatState {
    #[serde(default)]
    round: u32,
    #[serde(default)]
    damage_log: Vec<DamageEvent>,
    #[serde(default)]
    effects: HashMap<String, Vec<StatusEffect>>,
    /// Whether combat is currently active.
    #[serde(default)]
    in_combat: bool,
    /// Initiative order.
    #[serde(default)]
    turn_order: Vec<String>,
    /// Who is currently acting.
    #[serde(default)]
    current_turn: Option<String>,
    /// Actions available to the current player.
    #[serde(default)]
    available_actions: Vec<String>,
    /// Drama weight for pacing (story 2-7).
    #[serde(default)]
    drama_weight: f64,
}

impl CombatState {
    /// Create a new combat state starting at round 1.
    pub fn new() -> Self {
        Self {
            round: 1,
            damage_log: Vec::new(),
            effects: HashMap::new(),
            in_combat: false,
            turn_order: Vec::new(),
            current_turn: None,
            available_actions: Vec::new(),
            drama_weight: 0.0,
        }
    }

    /// Current combat round (starts at 1).
    pub fn round(&self) -> u32 {
        self.round
    }

    /// Advance to the next round.
    pub fn advance_round(&mut self) {
        let span = tracing::info_span!(
            "combat_advance_round",
            round_from = self.round,
            round_to = tracing::field::Empty,
        );
        let _guard = span.enter();
        self.round += 1;
        span.record("round_to", self.round);
    }

    /// The ordered log of damage events.
    pub fn damage_log(&self) -> &[DamageEvent] {
        &self.damage_log
    }

    /// Record a damage event.
    pub fn log_damage(&mut self, event: DamageEvent) {
        let span = tracing::info_span!(
            "combat_log_damage",
            attacker = %event.attacker,
            target = %event.target,
            damage = event.damage,
            round = event.round,
        );
        let _guard = span.enter();
        self.damage_log.push(event);
    }

    /// Add a status effect to a combatant.
    pub fn add_effect(&mut self, target: &str, effect: StatusEffect) {
        let span = tracing::info_span!(
            "combat_add_effect",
            target = target,
            effect_kind = ?effect.kind(),
            duration = effect.remaining_rounds(),
        );
        let _guard = span.enter();
        self.effects
            .entry(target.to_string())
            .or_default()
            .push(effect);
    }

    /// Get effects currently stored for a combatant.
    ///
    /// Call `tick_effects()` first to remove expired effects.
    pub fn effects_on(&self, target: &str) -> Vec<&StatusEffect> {
        self.effects
            .get(target)
            .map(|effs| effs.iter().collect())
            .unwrap_or_default()
    }

    /// Whether combat is currently active.
    pub fn in_combat(&self) -> bool {
        self.in_combat
    }

    /// Set whether combat is active.
    pub fn set_in_combat(&mut self, active: bool) {
        self.in_combat = active;
    }

    /// The initiative turn order.
    pub fn turn_order(&self) -> &[String] {
        &self.turn_order
    }

    /// Set the turn order.
    pub fn set_turn_order(&mut self, order: Vec<String>) {
        self.turn_order = order;
    }

    /// Who is currently acting.
    pub fn current_turn(&self) -> Option<&str> {
        self.current_turn.as_deref()
    }

    /// Set the current turn holder.
    pub fn set_current_turn(&mut self, turn: String) {
        self.current_turn = Some(turn);
    }

    /// Available player actions.
    pub fn available_actions(&self) -> &[String] {
        &self.available_actions
    }

    /// Set available actions.
    pub fn set_available_actions(&mut self, actions: Vec<String>) {
        self.available_actions = actions;
    }

    /// Drama weight for pacing.
    pub fn drama_weight(&self) -> f64 {
        self.drama_weight
    }

    /// Set drama weight.
    pub fn set_drama_weight(&mut self, weight: f64) {
        self.drama_weight = weight;
    }

    /// Start combat with the given combatant names.
    ///
    /// Sets `in_combat`, populates `turn_order`, and sets `current_turn` to
    /// the first combatant. No-op if `combatants` is empty or combat is
    /// already active.
    pub fn engage(&mut self, combatants: Vec<String>) {
        if combatants.is_empty() || self.in_combat {
            return;
        }
        let span = tracing::info_span!(
            "combat_engage",
            combatant_count = combatants.len(),
        );
        let _guard = span.enter();

        self.in_combat = true;
        self.current_turn = Some(combatants[0].clone());
        self.turn_order = combatants;
    }

    /// Resolve an attack from `attacker` against `target`.
    ///
    /// Uses level-scaled damage (via `level_to_damage`) and checks for
    /// Stun effects that prevent action. Returns a `RoundResult` with
    /// the damage events produced.
    pub fn resolve_attack(
        &mut self,
        attacker_name: &str,
        attacker: &impl Combatant,
        target_name: &str,
        _target: &impl Combatant,
    ) -> RoundResult {
        let span = tracing::info_span!(
            "combat_resolve_attack",
            attacker = attacker_name,
            target = target_name,
            round = self.round,
        );
        let _guard = span.enter();

        // Stunned combatants cannot act
        if self
            .effects_on(attacker_name)
            .iter()
            .any(|e| e.kind() == StatusEffectKind::Stun)
        {
            return RoundResult {
                round: self.round,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            };
        }

        let base_damage = 5;
        let damage = level_to_damage(base_damage, attacker.level());

        let event = DamageEvent {
            attacker: attacker_name.to_string(),
            target: target_name.to_string(),
            damage,
            round: self.round,
        };
        self.damage_log.push(event.clone());

        RoundResult {
            round: self.round,
            damage_events: vec![event],
            effects_applied: vec![],
            effects_expired: vec![],
        }
    }

    /// Check whether combat should end.
    ///
    /// Returns `Some(Victory)` if all enemies are dead, `Some(Defeat)` if
    /// all players are dead, or `None` if combat continues.
    pub fn check_victory(
        &self,
        players: &[&dyn Combatant],
        enemies: &[&dyn Combatant],
    ) -> Option<CombatOutcome> {
        if enemies.iter().all(|e| !e.is_alive()) {
            Some(CombatOutcome::Victory)
        } else if players.iter().all(|p| !p.is_alive()) {
            Some(CombatOutcome::Defeat)
        } else {
            None
        }
    }

    /// End combat and reset all state.
    ///
    /// Clears turn order, damage log, effects, and resets the round counter.
    pub fn disengage(&mut self) {
        let span = tracing::info_span!("combat_disengage", round = self.round);
        let _guard = span.enter();

        self.in_combat = false;
        self.round = 1;
        self.damage_log.clear();
        self.effects.clear();
        self.turn_order.clear();
        self.current_turn = None;
        self.available_actions.clear();
        self.drama_weight = 0.0;
    }

    /// Advance to the next combatant in turn order.
    ///
    /// Wraps around to the first combatant when the end is reached,
    /// advancing the round counter.
    pub fn advance_turn(&mut self) {
        if self.turn_order.is_empty() {
            return;
        }
        let current = self.current_turn.as_deref().unwrap_or("");
        let idx = self.turn_order.iter().position(|n| n == current).unwrap_or(0);
        let next_idx = (idx + 1) % self.turn_order.len();
        if next_idx == 0 {
            self.advance_round();
        }
        self.current_turn = Some(self.turn_order[next_idx].clone());
    }

    /// Tick all effects (decrement durations) and remove expired ones.
    pub fn tick_effects(&mut self) {
        let span = tracing::info_span!(
            "combat_tick_effects",
            effects_expired = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut expired_count: u64 = 0;
        for effects in self.effects.values_mut() {
            for effect in effects.iter_mut() {
                effect.tick();
            }
            let before = effects.len();
            effects.retain(|e| !e.is_expired());
            expired_count += (before - effects.len()) as u64;
        }
        span.record("effects_expired", expired_count);
    }
}

impl Default for CombatState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single damage event in combat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageEvent {
    /// Who dealt the damage.
    pub attacker: String,
    /// Who received the damage.
    pub target: String,
    /// Amount of damage dealt.
    pub damage: i32,
    /// Which round this occurred in.
    pub round: u32,
}

/// The result of resolving a combat round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundResult {
    /// Which round was resolved.
    pub round: u32,
    /// Damage events that occurred.
    pub damage_events: Vec<DamageEvent>,
    /// Status effects that were applied this round.
    pub effects_applied: Vec<String>,
    /// Status effects that expired this round.
    pub effects_expired: Vec<String>,
}

/// A status effect applied to a combatant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEffect {
    kind: StatusEffectKind,
    remaining_rounds: u32,
}

impl StatusEffect {
    /// Create a new status effect with a duration in rounds.
    pub fn new(kind: StatusEffectKind, duration: u32) -> Self {
        Self {
            kind,
            remaining_rounds: duration,
        }
    }

    /// The type of effect.
    pub fn kind(&self) -> StatusEffectKind {
        self.kind
    }

    /// How many rounds remain.
    pub fn remaining_rounds(&self) -> u32 {
        self.remaining_rounds
    }

    /// Whether the effect has expired (0 rounds remaining).
    pub fn is_expired(&self) -> bool {
        self.remaining_rounds == 0
    }

    /// Decrement the duration by one round (floors at 0).
    pub fn tick(&mut self) {
        self.remaining_rounds = self.remaining_rounds.saturating_sub(1);
    }
}

/// The kinds of status effects that can be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StatusEffectKind {
    /// Damage over time.
    Poison,
    /// Cannot act.
    Stun,
    /// Bonus to rolls.
    Bless,
    /// Penalty to rolls.
    Curse,
}

/// The outcome of a combat encounter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CombatOutcome {
    /// All enemies defeated.
    Victory,
    /// All players defeated.
    Defeat,
}
