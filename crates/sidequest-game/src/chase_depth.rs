//! Chase System Depth — C1 through C5 enrichment modules.
//!
//! C1: Rig Damage (HP, archetypes, damage tiers, fuel)
//! C2: Multi-Actor Roles (Driver, Gunner, Mechanic, Support)
//! C3: Beat System (phases, decisions, outcomes)
//! C4: Terrain Modifiers (danger escalation, speed/maneuver penalties)
//! C5: Chase Cinematography (camera modes, drama-driven prose length)

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// C1 — Rig Damage
// ---------------------------------------------------------------------------

/// Rig archetype determines base stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RigType {
    /// Fast, fragile interceptor.
    Interceptor,
    /// Heavy armored war rig.
    WarRig,
    /// Light, maneuverable bike.
    Bike,
    /// Slow, high-fuel hauler.
    Hauler,
    /// Cobbled-together frankenstein.
    Frankenstein,
}

impl RigType {
    /// Base stats for this archetype: (hp, speed, armor, maneuver, fuel).
    pub fn base_stats(self) -> (i32, i32, i32, i32, i32) {
        match self {
            RigType::Interceptor => (15, 5, 1, 3, 8),
            RigType::WarRig => (30, 2, 5, 1, 12),
            RigType::Bike => (8, 4, 0, 5, 5),
            RigType::Hauler => (25, 2, 3, 1, 20),
            RigType::Frankenstein => (18, 3, 2, 3, 10),
        }
    }
}

/// Rig damage tier derived from HP percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RigDamageTier {
    /// >75% HP
    Pristine,
    /// 51-75% HP
    Cosmetic,
    /// 26-50% HP
    Failing,
    /// 1-25% HP
    Skeleton,
    /// 0% HP
    Wreck,
}

impl fmt::Display for RigDamageTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RigDamageTier::Pristine => write!(f, "PRISTINE"),
            RigDamageTier::Cosmetic => write!(f, "COSMETIC"),
            RigDamageTier::Failing => write!(f, "FAILING"),
            RigDamageTier::Skeleton => write!(f, "SKELETON"),
            RigDamageTier::Wreck => write!(f, "WRECK"),
        }
    }
}

/// Mechanical stats for a chase rig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigStats {
    /// Current hit points.
    pub rig_hp: i32,
    /// Maximum hit points.
    pub max_rig_hp: i32,
    /// Base speed.
    pub speed: i32,
    /// Damage reduction.
    pub armor: i32,
    /// Handling/dodge capability.
    pub maneuver: i32,
    /// Current fuel.
    pub fuel: i32,
    /// Maximum fuel.
    pub max_fuel: i32,
    /// Archetype.
    pub rig_type: RigType,
}

impl RigStats {
    /// Create a rig with base stats from archetype.
    pub fn from_type(rig_type: RigType) -> Self {
        let (hp, speed, armor, maneuver, fuel) = rig_type.base_stats();
        Self {
            rig_hp: hp,
            max_rig_hp: hp,
            speed,
            armor,
            maneuver,
            fuel,
            max_fuel: fuel,
            rig_type,
        }
    }

    /// Current damage tier based on HP percentage.
    pub fn damage_tier(&self) -> RigDamageTier {
        if self.max_rig_hp == 0 {
            return RigDamageTier::Wreck;
        }
        let pct = (self.rig_hp as f64 / self.max_rig_hp as f64) * 100.0;
        if pct <= 0.0 {
            RigDamageTier::Wreck
        } else if pct <= 25.0 {
            RigDamageTier::Skeleton
        } else if pct <= 50.0 {
            RigDamageTier::Failing
        } else if pct <= 75.0 {
            RigDamageTier::Cosmetic
        } else {
            RigDamageTier::Pristine
        }
    }

    /// Apply damage (reduced by armor), clamp HP >= 0.
    /// Returns (actual_damage, old_tier, new_tier) for tier-crossing detection.
    pub fn apply_damage(&mut self, raw_damage: i32) -> (i32, RigDamageTier, RigDamageTier) {
        let old_tier = self.damage_tier();
        let effective = (raw_damage - self.armor).max(0);
        self.rig_hp = (self.rig_hp - effective).max(0);
        let new_tier = self.damage_tier();
        (effective, old_tier, new_tier)
    }

    /// Repair HP, clamped to max.
    pub fn repair(&mut self, amount: i32) {
        self.rig_hp = (self.rig_hp + amount).min(self.max_rig_hp);
    }

    /// Consume fuel. Returns false if already at 0.
    pub fn consume_fuel(&mut self, amount: i32) -> bool {
        if self.fuel <= 0 {
            return false;
        }
        self.fuel = (self.fuel - amount).max(0);
        true
    }

    /// True if fuel is at or below 10% of max.
    pub fn fuel_warning(&self) -> bool {
        if self.max_fuel == 0 {
            return true;
        }
        (self.fuel as f64 / self.max_fuel as f64) <= 0.10
    }

    /// True if rig is destroyed.
    pub fn is_wrecked(&self) -> bool {
        self.rig_hp <= 0
    }
}

// ---------------------------------------------------------------------------
// C2 — Multi-Actor Roles
// ---------------------------------------------------------------------------

/// Role a character can fill during a chase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChaseRole {
    /// Mandatory. Controls route and separation delta.
    Driver,
    /// Engages pursuers, deals damage.
    Gunner,
    /// Patches rig HP between/during beats.
    Mechanic,
    /// Morale, supplies, prevents attrition.
    Support,
}

impl fmt::Display for ChaseRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChaseRole::Driver => write!(f, "Driver"),
            ChaseRole::Gunner => write!(f, "Gunner"),
            ChaseRole::Mechanic => write!(f, "Mechanic"),
            ChaseRole::Support => write!(f, "Support"),
        }
    }
}

/// A character assigned to a chase role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaseActor {
    /// Character name.
    pub name: String,
    /// Assigned role.
    pub role: ChaseRole,
}

// ---------------------------------------------------------------------------
// C3 — Beat System
// ---------------------------------------------------------------------------

/// Chase phase — narrative arc of the chase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChasePhase {
    /// Initial setup.
    Setup,
    /// Chase begins.
    Opening,
    /// Tension rises.
    Escalation,
    /// Peak intensity.
    Climax,
    /// Winding down.
    Resolution,
}

impl ChasePhase {
    /// Drama weight for this phase (used by cinematography).
    pub fn drama_weight(self) -> f64 {
        match self {
            ChasePhase::Setup => 0.70,
            ChasePhase::Opening => 0.75,
            ChasePhase::Escalation => 0.80,
            ChasePhase::Climax => 0.95,
            ChasePhase::Resolution => 0.70,
        }
    }
}

impl fmt::Display for ChasePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChasePhase::Setup => write!(f, "SETUP"),
            ChasePhase::Opening => write!(f, "OPENING"),
            ChasePhase::Escalation => write!(f, "ESCALATION"),
            ChasePhase::Climax => write!(f, "CLIMAX"),
            ChasePhase::Resolution => write!(f, "RESOLUTION"),
        }
    }
}

/// Outcome of the chase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChaseOutcome {
    /// Separation >= goal.
    Escape,
    /// Separation == 0.
    Caught,
    /// Rig HP == 0.
    Crashed,
    /// Players gave up.
    Abandoned,
}

/// A decision option presented during a beat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatDecision {
    /// Human-readable description of the option.
    pub description: String,
    /// Separation change if chosen.
    pub separation_delta: i32,
    /// Risk description for narrator.
    pub risk: String,
}

/// A single beat in the chase sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaseBeat {
    /// Beat number (1-indexed).
    pub beat_number: u32,
    /// Phase during this beat.
    pub phase: ChasePhase,
    /// Available decisions (2-3 options).
    pub decisions: Vec<BeatDecision>,
    /// Terrain danger level for this beat.
    pub terrain_danger: u32,
}

/// Determine the chase phase for a given beat number and whether outcome is resolved.
pub fn phase_for_beat(beat: u32, outcome_resolved: bool) -> ChasePhase {
    if outcome_resolved {
        return ChasePhase::Resolution;
    }
    match beat {
        0 => ChasePhase::Setup,
        1 => ChasePhase::Opening,
        2..=4 => ChasePhase::Escalation,
        // Beat 5 is border; spec says ESCALATION→CLIMAX at beat 6 or by drama_weight.
        // We transition at beat 5 (0-indexed beat 6 from spec's 1-indexed system).
        5 => ChasePhase::Climax,
        _ => ChasePhase::Climax,
    }
}

/// Check outcome conditions.
pub fn check_outcome(separation: i32, goal: i32, rig_hp: i32) -> Option<ChaseOutcome> {
    if rig_hp <= 0 {
        Some(ChaseOutcome::Crashed)
    } else if separation <= 0 {
        Some(ChaseOutcome::Caught)
    } else if separation >= goal {
        Some(ChaseOutcome::Escape)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// C4 — Terrain Modifiers
// ---------------------------------------------------------------------------

/// Terrain modifiers computed from danger level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TerrainModifiers {
    /// Danger level (0-5).
    pub danger: u32,
    /// Speed penalty (negative).
    pub speed_modifier: i32,
    /// Maneuver penalty (negative).
    pub maneuver_modifier: i32,
    /// Automatic rig damage per beat.
    pub rig_damage_per_beat: i32,
    /// Whether a terrain decision point should trigger.
    pub terrain_decision: bool,
}

/// Compute danger level from beat number.
pub fn danger_for_beat(beat: u32, phase: ChasePhase) -> u32 {
    if phase == ChasePhase::Climax {
        return 5;
    }
    match beat {
        0..=1 => beat.min(1),
        2..=3 => 2,
        4..=5 => 3,
        _ => 4,
    }
}

/// Compute terrain modifiers from a danger level.
pub fn terrain_modifiers(danger: u32) -> TerrainModifiers {
    let speed_modifier = -(danger as i32 / 2);
    let maneuver_modifier = if danger >= 2 { -(danger as i32 / 3) } else { 0 };
    let rig_damage_per_beat = (danger as i32 - 2).max(0);
    let terrain_decision = danger >= 3;

    TerrainModifiers {
        danger,
        speed_modifier,
        maneuver_modifier,
        rig_damage_per_beat,
        terrain_decision,
    }
}

/// Apply terrain modifiers to rig stats (returns effective speed and maneuver).
pub fn apply_terrain_to_rig(rig: &RigStats, mods: &TerrainModifiers) -> (i32, i32) {
    let effective_speed = (rig.speed + mods.speed_modifier).max(0);
    let effective_maneuver = (rig.maneuver + mods.maneuver_modifier).max(0);
    (effective_speed, effective_maneuver)
}

// ---------------------------------------------------------------------------
// C5 — Chase Cinematography
// ---------------------------------------------------------------------------

/// Camera mode for a chase phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraMode {
    /// Wide establishing shot.
    WideEstablishing,
    /// Wide tracking shot.
    WideTracking,
    /// Tight tracking shot.
    TightTracking,
    /// Close-up, slow-motion.
    CloseUpSlowMotion,
    /// Wide pull-back.
    WidePullBack,
}

impl fmt::Display for CameraMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CameraMode::WideEstablishing => write!(f, "Wide establishing"),
            CameraMode::WideTracking => write!(f, "Wide tracking"),
            CameraMode::TightTracking => write!(f, "Tight tracking"),
            CameraMode::CloseUpSlowMotion => write!(f, "Close-up, slow-motion"),
            CameraMode::WidePullBack => write!(f, "Wide pull-back"),
        }
    }
}

/// Cinematography cues for narrator prompt injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaseCinematography {
    /// Camera mode for this phase.
    pub camera: CameraMode,
    /// Pacing description.
    pub pace: String,
    /// Style instruction.
    pub style: String,
    /// Target sentence count range (min, max).
    pub sentence_range: (u32, u32),
}

/// Map a chase phase to its camera mode.
pub fn camera_for_phase(phase: ChasePhase) -> CameraMode {
    match phase {
        ChasePhase::Setup => CameraMode::WideEstablishing,
        ChasePhase::Opening => CameraMode::WideTracking,
        ChasePhase::Escalation => CameraMode::TightTracking,
        ChasePhase::Climax => CameraMode::CloseUpSlowMotion,
        ChasePhase::Resolution => CameraMode::WidePullBack,
    }
}

/// Map drama weight to sentence count range.
pub fn sentence_range_for_drama(weight: f64) -> (u32, u32) {
    if weight < 0.30 {
        (1, 2)
    } else if weight < 0.50 {
        (2, 3)
    } else if weight < 0.70 {
        (3, 4)
    } else {
        (4, 6)
    }
}

/// Full cinematography cues for a phase.
pub fn cinematography_for_phase(phase: ChasePhase) -> ChaseCinematography {
    let camera = camera_for_phase(phase);
    let weight = phase.drama_weight();
    let sentence_range = sentence_range_for_drama(weight);

    let (pace, style) = match phase {
        ChasePhase::Setup => (
            "Slow, anticipatory".to_string(),
            "Set the scene".to_string(),
        ),
        ChasePhase::Opening => (
            "Brisk, building".to_string(),
            "Establish geography".to_string(),
        ),
        ChasePhase::Escalation => (
            "Rapid, breathless".to_string(),
            "Rising tension, short sentences".to_string(),
        ),
        ChasePhase::Climax => (
            "Peak intensity".to_string(),
            "Full cinematic treatment".to_string(),
        ),
        ChasePhase::Resolution => (
            "Slowing, settling".to_string(),
            "Let tension drain".to_string(),
        ),
    };

    ChaseCinematography {
        camera,
        pace,
        style,
        sentence_range,
    }
}

// ---------------------------------------------------------------------------
// Narrator Context Formatting
// ---------------------------------------------------------------------------

/// Format complete chase context for narrator prompt injection.
pub fn format_chase_context(
    beat: &ChaseBeat,
    rig: &RigStats,
    actors: &[ChaseActor],
    separation: i32,
    goal: i32,
) -> String {
    let mut lines = Vec::new();

    lines.push("[CHASE SEQUENCE]".to_string());

    // Phase & beat
    lines.push(format!(
        "Phase: {} | Beat: {} | Separation: {}/{}",
        beat.phase, beat.beat_number, separation, goal
    ));

    // Rig status (C1)
    let tier = rig.damage_tier();
    lines.push(format!(
        "Rig: {:?} — HP: {}/{} ({}) | Speed: {} | Armor: {} | Maneuver: {} | Fuel: {}/{}",
        rig.rig_type,
        rig.rig_hp,
        rig.max_rig_hp,
        tier,
        rig.speed,
        rig.armor,
        rig.maneuver,
        rig.fuel,
        rig.max_fuel,
    ));
    if rig.fuel_warning() {
        lines.push("WARNING: Running low on fuel!".to_string());
    }

    // Terrain (C4)
    let mods = terrain_modifiers(beat.terrain_danger);
    let (eff_speed, eff_maneuver) = apply_terrain_to_rig(rig, &mods);
    lines.push(format!(
        "Terrain danger: {} | Effective speed: {} | Effective maneuver: {}",
        mods.danger, eff_speed, eff_maneuver,
    ));
    if mods.rig_damage_per_beat > 0 {
        lines.push(format!(
            "Terrain damage: {} per beat",
            mods.rig_damage_per_beat
        ));
    }

    // Crew roles (C2)
    if !actors.is_empty() {
        let roles: Vec<String> = actors
            .iter()
            .map(|a| format!("{} ({})", a.name, a.role))
            .collect();
        lines.push(format!("Crew: {}", roles.join(", ")));
    }

    // Decisions (C3)
    if !beat.decisions.is_empty() {
        lines.push("Decisions:".to_string());
        for (i, d) in beat.decisions.iter().enumerate() {
            lines.push(format!(
                "  {}. {} [sep {:+}, risk: {}]",
                i + 1,
                d.description,
                d.separation_delta,
                d.risk,
            ));
        }
    }

    // Cinematography (C5)
    let cine = cinematography_for_phase(beat.phase);
    lines.push(format!(
        "Camera: {} | Pace: {} | Style: {} | Sentences: {}-{}",
        cine.camera, cine.pace, cine.style, cine.sentence_range.0, cine.sentence_range.1,
    ));

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- C1: Rig Damage --

    #[test]
    fn rig_from_type_has_correct_base_stats() {
        let rig = RigStats::from_type(RigType::Interceptor);
        assert_eq!(rig.rig_hp, 15);
        assert_eq!(rig.max_rig_hp, 15);
        assert_eq!(rig.speed, 5);
        assert_eq!(rig.armor, 1);
        assert_eq!(rig.maneuver, 3);
        assert_eq!(rig.fuel, 8);
        assert_eq!(rig.max_fuel, 8);
    }

    #[test]
    fn rig_damage_tiers() {
        let mut rig = RigStats::from_type(RigType::WarRig); // 30 HP
        assert_eq!(rig.damage_tier(), RigDamageTier::Pristine);

        rig.rig_hp = 23; // 76.7% — still pristine
        assert_eq!(rig.damage_tier(), RigDamageTier::Pristine);

        rig.rig_hp = 22; // 73.3% — cosmetic
        assert_eq!(rig.damage_tier(), RigDamageTier::Cosmetic);

        rig.rig_hp = 15; // 50% — failing
        assert_eq!(rig.damage_tier(), RigDamageTier::Failing);

        rig.rig_hp = 7; // 23.3% — skeleton
        assert_eq!(rig.damage_tier(), RigDamageTier::Skeleton);

        rig.rig_hp = 0; // wreck
        assert_eq!(rig.damage_tier(), RigDamageTier::Wreck);
    }

    #[test]
    fn rig_damage_reduced_by_armor() {
        let mut rig = RigStats::from_type(RigType::WarRig); // armor 5
        let (actual, old, new) = rig.apply_damage(8);
        assert_eq!(actual, 3); // 8 - 5 armor
        assert_eq!(rig.rig_hp, 27);
        assert_eq!(old, RigDamageTier::Pristine);
        assert_eq!(new, RigDamageTier::Pristine);
    }

    #[test]
    fn rig_damage_clamps_to_zero() {
        let mut rig = RigStats::from_type(RigType::Bike); // 8 HP, 0 armor
        rig.apply_damage(100);
        assert_eq!(rig.rig_hp, 0);
        assert!(rig.is_wrecked());
    }

    #[test]
    fn rig_repair_clamps_to_max() {
        let mut rig = RigStats::from_type(RigType::Hauler); // 25 HP
        rig.rig_hp = 10;
        rig.repair(50);
        assert_eq!(rig.rig_hp, 25);
    }

    #[test]
    fn fuel_warning_at_ten_percent() {
        let mut rig = RigStats::from_type(RigType::Frankenstein); // 10 fuel
        rig.fuel = 1; // 10%
        assert!(rig.fuel_warning());

        rig.fuel = 2; // 20%
        assert!(!rig.fuel_warning());
    }

    #[test]
    fn fuel_consume_returns_false_at_zero() {
        let mut rig = RigStats::from_type(RigType::Bike);
        rig.fuel = 0;
        assert!(!rig.consume_fuel(1));
    }

    // -- C2: Multi-Actor Roles --

    #[test]
    fn chase_role_display() {
        assert_eq!(format!("{}", ChaseRole::Driver), "Driver");
        assert_eq!(format!("{}", ChaseRole::Gunner), "Gunner");
        assert_eq!(format!("{}", ChaseRole::Mechanic), "Mechanic");
        assert_eq!(format!("{}", ChaseRole::Support), "Support");
    }

    // -- C3: Beat System --

    #[test]
    fn phase_transitions_by_beat() {
        assert_eq!(phase_for_beat(0, false), ChasePhase::Setup);
        assert_eq!(phase_for_beat(1, false), ChasePhase::Opening);
        assert_eq!(phase_for_beat(2, false), ChasePhase::Escalation);
        assert_eq!(phase_for_beat(4, false), ChasePhase::Escalation);
        assert_eq!(phase_for_beat(5, false), ChasePhase::Climax);
        assert_eq!(phase_for_beat(7, false), ChasePhase::Climax);
    }

    #[test]
    fn resolved_always_resolution() {
        assert_eq!(phase_for_beat(2, true), ChasePhase::Resolution);
        assert_eq!(phase_for_beat(5, true), ChasePhase::Resolution);
    }

    #[test]
    fn outcome_escape() {
        assert_eq!(check_outcome(10, 10, 5), Some(ChaseOutcome::Escape));
    }

    #[test]
    fn outcome_caught() {
        assert_eq!(check_outcome(0, 10, 5), Some(ChaseOutcome::Caught));
    }

    #[test]
    fn outcome_crashed() {
        // Crashed takes priority over caught (HP checked first).
        assert_eq!(check_outcome(0, 10, 0), Some(ChaseOutcome::Crashed));
    }

    #[test]
    fn outcome_none_when_in_progress() {
        assert_eq!(check_outcome(5, 10, 10), None);
    }

    // -- C4: Terrain Modifiers --

    #[test]
    fn danger_escalation_by_beat() {
        assert_eq!(danger_for_beat(0, ChasePhase::Setup), 0);
        assert_eq!(danger_for_beat(1, ChasePhase::Opening), 1);
        assert_eq!(danger_for_beat(2, ChasePhase::Escalation), 2);
        assert_eq!(danger_for_beat(3, ChasePhase::Escalation), 2);
        assert_eq!(danger_for_beat(4, ChasePhase::Escalation), 3);
        assert_eq!(danger_for_beat(5, ChasePhase::Climax), 5); // CLIMAX override
        assert_eq!(danger_for_beat(7, ChasePhase::Climax), 5);
    }

    #[test]
    fn terrain_modifier_formulas() {
        let m0 = terrain_modifiers(0);
        assert_eq!(m0.speed_modifier, 0);
        assert_eq!(m0.maneuver_modifier, 0);
        assert_eq!(m0.rig_damage_per_beat, 0);
        assert!(!m0.terrain_decision);

        let m2 = terrain_modifiers(2);
        assert_eq!(m2.speed_modifier, -1);
        assert_eq!(m2.maneuver_modifier, 0); // 2/3 = 0 in integer division
        assert_eq!(m2.rig_damage_per_beat, 0);
        assert!(!m2.terrain_decision);

        let m3 = terrain_modifiers(3);
        assert_eq!(m3.speed_modifier, -1);
        assert_eq!(m3.maneuver_modifier, -1);
        assert_eq!(m3.rig_damage_per_beat, 1);
        assert!(m3.terrain_decision);

        let m5 = terrain_modifiers(5);
        assert_eq!(m5.speed_modifier, -2);
        assert_eq!(m5.maneuver_modifier, -1);
        assert_eq!(m5.rig_damage_per_beat, 3);
        assert!(m5.terrain_decision);
    }

    #[test]
    fn terrain_applied_to_rig() {
        let rig = RigStats::from_type(RigType::Interceptor); // speed 5, maneuver 3
        let mods = terrain_modifiers(5); // speed -2, maneuver -1
        let (speed, maneuver) = apply_terrain_to_rig(&rig, &mods);
        assert_eq!(speed, 3);
        assert_eq!(maneuver, 2);
    }

    #[test]
    fn terrain_doesnt_go_negative() {
        let rig = RigStats::from_type(RigType::Hauler); // speed 2, maneuver 1
        let mods = terrain_modifiers(5); // speed -2, maneuver -1
        let (speed, maneuver) = apply_terrain_to_rig(&rig, &mods);
        assert_eq!(speed, 0);
        assert_eq!(maneuver, 0);
    }

    // -- C5: Cinematography --

    #[test]
    fn camera_modes_per_phase() {
        assert_eq!(
            camera_for_phase(ChasePhase::Setup),
            CameraMode::WideEstablishing
        );
        assert_eq!(
            camera_for_phase(ChasePhase::Opening),
            CameraMode::WideTracking
        );
        assert_eq!(
            camera_for_phase(ChasePhase::Escalation),
            CameraMode::TightTracking
        );
        assert_eq!(
            camera_for_phase(ChasePhase::Climax),
            CameraMode::CloseUpSlowMotion
        );
        assert_eq!(
            camera_for_phase(ChasePhase::Resolution),
            CameraMode::WidePullBack
        );
    }

    #[test]
    fn sentence_ranges_by_drama() {
        assert_eq!(sentence_range_for_drama(0.0), (1, 2));
        assert_eq!(sentence_range_for_drama(0.29), (1, 2));
        assert_eq!(sentence_range_for_drama(0.30), (2, 3));
        assert_eq!(sentence_range_for_drama(0.49), (2, 3));
        assert_eq!(sentence_range_for_drama(0.50), (3, 4));
        assert_eq!(sentence_range_for_drama(0.69), (3, 4));
        assert_eq!(sentence_range_for_drama(0.70), (4, 6));
        assert_eq!(sentence_range_for_drama(1.0), (4, 6));
    }

    #[test]
    fn cinematography_climax_is_intense() {
        let cine = cinematography_for_phase(ChasePhase::Climax);
        assert_eq!(cine.camera, CameraMode::CloseUpSlowMotion);
        assert_eq!(cine.sentence_range, (4, 6)); // 0.95 drama weight
        assert_eq!(cine.pace, "Peak intensity");
    }

    // -- Integration: format_chase_context --

    #[test]
    fn format_chase_context_produces_all_sections() {
        let beat = ChaseBeat {
            beat_number: 3,
            phase: ChasePhase::Escalation,
            decisions: vec![
                BeatDecision {
                    description: "Floor it through the gap".to_string(),
                    separation_delta: 2,
                    risk: "high damage".to_string(),
                },
                BeatDecision {
                    description: "Take the side road".to_string(),
                    separation_delta: -1,
                    risk: "low".to_string(),
                },
            ],
            terrain_danger: 3,
        };
        let rig = RigStats::from_type(RigType::Interceptor);
        let actors = vec![
            ChaseActor {
                name: "Max".to_string(),
                role: ChaseRole::Driver,
            },
            ChaseActor {
                name: "Furiosa".to_string(),
                role: ChaseRole::Gunner,
            },
        ];

        let ctx = format_chase_context(&beat, &rig, &actors, 5, 10);

        assert!(ctx.contains("[CHASE SEQUENCE]"));
        assert!(ctx.contains("ESCALATION"));
        assert!(ctx.contains("Beat: 3"));
        assert!(ctx.contains("Separation: 5/10"));
        assert!(ctx.contains("Interceptor"));
        assert!(ctx.contains("HP: 15/15"));
        assert!(ctx.contains("PRISTINE"));
        assert!(ctx.contains("Terrain danger: 3"));
        assert!(ctx.contains("Terrain damage: 1 per beat"));
        assert!(ctx.contains("Max (Driver)"));
        assert!(ctx.contains("Furiosa (Gunner)"));
        assert!(ctx.contains("Floor it through the gap"));
        assert!(ctx.contains("[sep +2, risk: high damage]"));
        assert!(ctx.contains("Tight tracking"));
    }

    #[test]
    fn format_chase_context_fuel_warning() {
        let beat = ChaseBeat {
            beat_number: 1,
            phase: ChasePhase::Opening,
            decisions: vec![],
            terrain_danger: 0,
        };
        let mut rig = RigStats::from_type(RigType::WarRig); // 12 fuel
        rig.fuel = 1; // ~8%, below 10%

        let ctx = format_chase_context(&beat, &rig, &[], 3, 10);
        assert!(ctx.contains("Running low on fuel"));
    }

    // -- All 5 rig archetypes --

    #[test]
    fn all_archetypes_have_correct_stats() {
        let specs: Vec<(RigType, i32, i32, i32, i32, i32)> = vec![
            (RigType::Interceptor, 15, 5, 1, 3, 8),
            (RigType::WarRig, 30, 2, 5, 1, 12),
            (RigType::Bike, 8, 4, 0, 5, 5),
            (RigType::Hauler, 25, 2, 3, 1, 20),
            (RigType::Frankenstein, 18, 3, 2, 3, 10),
        ];

        for (rt, hp, speed, armor, maneuver, fuel) in specs {
            let rig = RigStats::from_type(rt);
            assert_eq!(rig.max_rig_hp, hp, "{:?} HP", rt);
            assert_eq!(rig.speed, speed, "{:?} speed", rt);
            assert_eq!(rig.armor, armor, "{:?} armor", rt);
            assert_eq!(rig.maneuver, maneuver, "{:?} maneuver", rt);
            assert_eq!(rig.max_fuel, fuel, "{:?} fuel", rt);
        }
    }

    #[test]
    fn damage_tier_crosses_emit_different_tiers() {
        let mut rig = RigStats::from_type(RigType::Frankenstein); // 18 HP, 2 armor
        assert_eq!(rig.damage_tier(), RigDamageTier::Pristine);

        // Take 7 raw = 5 effective → 13 HP (72%) → Cosmetic
        let (_, old, new) = rig.apply_damage(7);
        assert_eq!(old, RigDamageTier::Pristine);
        assert_eq!(new, RigDamageTier::Cosmetic);
    }
}
