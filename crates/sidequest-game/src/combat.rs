//! Combat state — round tracking, damage log, status effects.
//!
//! Decomposes the Python GameState's combat handling into a focused type
//! that owns its own mutations (port lesson #4).

use std::collections::HashMap;

/// Tracks the state of an active combat encounter.
pub struct CombatState {
    round: u32,
    damage_log: Vec<DamageEvent>,
    effects: HashMap<String, Vec<StatusEffect>>,
}

impl CombatState {
    /// Create a new combat state starting at round 1.
    pub fn new() -> Self {
        Self {
            round: 1,
            damage_log: Vec::new(),
            effects: HashMap::new(),
        }
    }

    /// Current combat round (starts at 1).
    pub fn round(&self) -> u32 {
        self.round
    }

    /// Advance to the next round.
    pub fn advance_round(&mut self) {
        self.round += 1;
    }

    /// The ordered log of damage events.
    pub fn damage_log(&self) -> &[DamageEvent] {
        &self.damage_log
    }

    /// Record a damage event.
    pub fn log_damage(&mut self, event: DamageEvent) {
        self.damage_log.push(event);
    }

    /// Add a status effect to a combatant.
    pub fn add_effect(&mut self, target: &str, effect: StatusEffect) {
        self.effects
            .entry(target.to_string())
            .or_default()
            .push(effect);
    }

    /// Get active (non-expired) effects on a combatant.
    pub fn effects_on(&self, target: &str) -> Vec<&StatusEffect> {
        self.effects
            .get(target)
            .map(|effs| effs.iter().collect())
            .unwrap_or_default()
    }

    /// Tick all effects (decrement durations) and remove expired ones.
    pub fn tick_effects(&mut self) {
        for effects in self.effects.values_mut() {
            for effect in effects.iter_mut() {
                effect.tick();
            }
            effects.retain(|e| !e.is_expired());
        }
    }
}

impl Default for CombatState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single damage event in combat.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
