//! Export Rust-canonical TensionTracker fixtures for Python parity testing.
//!
//! Produces JSON files in `sidequest-server/tests/fixtures/tension/` that
//! capture the exact behaviour of `classify_round`, `classify_combat_outcome`,
//! and multi-round `TensionTracker` scenarios. The Python port asserts byte-
//! identical output against these fixtures.
//!
//! Run: `cargo run --example tension_fixture_export`
//! Optional override: `cargo run --example tension_fixture_export -- /custom/path`

use serde_json::{json, Value};
use sidequest_game::tension_tracker::{
    classify_combat_outcome, classify_round, CombatEvent, DamageEvent, DeliveryMode,
    DetailedCombatEvent, RoundResult, TensionTracker, TurnClassification,
};
use sidequest_genre::DramaThresholds;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn default_output_dir() -> PathBuf {
    // Workspace root is two levels up from this crate.
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("..")
        .join("..")
        .join("..")
        .join("sidequest-server")
        .join("tests")
        .join("fixtures")
        .join("tension")
}

fn combat_event_str(e: &CombatEvent) -> &'static str {
    match e {
        CombatEvent::Boring => "Boring",
        CombatEvent::Dramatic => "Dramatic",
        CombatEvent::Normal => "Normal",
    }
}

fn detailed_event_str(e: &DetailedCombatEvent) -> &'static str {
    match e {
        DetailedCombatEvent::CriticalHit => "CriticalHit",
        DetailedCombatEvent::KillingBlow => "KillingBlow",
        DetailedCombatEvent::DeathSave => "DeathSave",
        DetailedCombatEvent::FirstBlood => "FirstBlood",
        DetailedCombatEvent::NearMiss => "NearMiss",
        DetailedCombatEvent::LastStanding => "LastStanding",
        // DetailedCombatEvent is #[non_exhaustive]; this exporter only emits
        // documented variants. Adding a new variant must come with a
        // matching string label here.
        _ => panic!("tension_fixture_export: unknown DetailedCombatEvent variant — add a label"),
    }
}

fn classification_json(c: &TurnClassification) -> Value {
    match c {
        TurnClassification::Boring => json!({"kind": "Boring"}),
        TurnClassification::Normal => json!({"kind": "Normal"}),
        TurnClassification::Dramatic(e) => {
            json!({"kind": "Dramatic", "event": detailed_event_str(e)})
        }
    }
}

fn delivery_mode_str(m: &DeliveryMode) -> &'static str {
    match m {
        DeliveryMode::Instant => "Instant",
        DeliveryMode::Sentence => "Sentence",
        DeliveryMode::Streaming => "Streaming",
        // DeliveryMode is #[non_exhaustive]; new variants must add a label.
        _ => panic!("tension_fixture_export: unknown DeliveryMode variant — add a label"),
    }
}

fn round_json(round: &RoundResult, killed: Option<&str>) -> Value {
    json!({
        "round": round.round,
        "damage_events": round.damage_events.iter().map(|d| json!({
            "attacker": d.attacker,
            "target": d.target,
            "damage": d.damage,
            "round": d.round,
        })).collect::<Vec<_>>(),
        "effects_applied": round.effects_applied,
        "effects_expired": round.effects_expired,
        "killed": killed,
    })
}

fn dmg(attacker: &str, target: &str, damage: i32, round: u32) -> DamageEvent {
    DamageEvent {
        attacker: attacker.to_string(),
        target: target.to_string(),
        damage,
        round,
    }
}

fn classify_round_cases() -> Value {
    // Each case: an input round + killed value; output is the CombatEvent name.
    let cases: Vec<(&str, RoundResult, Option<&str>)> = vec![
        (
            "kill_is_dramatic_even_with_no_damage",
            RoundResult {
                round: 1,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            Some("orc"),
        ),
        (
            "kill_with_empty_string_is_still_dramatic",
            RoundResult {
                round: 2,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            Some(""),
        ),
        (
            "killed_none_no_events_no_damage_is_boring",
            RoundResult {
                round: 3,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "effects_applied_makes_dramatic",
            RoundResult {
                round: 4,
                damage_events: vec![],
                effects_applied: vec!["poisoned".into()],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "high_damage_at_threshold_is_dramatic",
            RoundResult {
                round: 5,
                damage_events: vec![dmg("hero", "orc", 15, 5)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "high_damage_above_threshold_is_dramatic",
            RoundResult {
                round: 6,
                damage_events: vec![dmg("hero", "orc", 30, 6)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "low_damage_is_normal",
            RoundResult {
                round: 7,
                damage_events: vec![dmg("hero", "orc", 5, 7)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "negative_damage_clamped_to_zero_so_boring",
            RoundResult {
                round: 8,
                damage_events: vec![dmg("hero", "orc", -10, 8)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "multiple_small_damage_summed_to_dramatic",
            RoundResult {
                round: 9,
                damage_events: vec![
                    dmg("hero", "orc", 5, 9),
                    dmg("rogue", "orc", 5, 9),
                    dmg("mage", "orc", 5, 9),
                ],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "multiple_small_damage_below_threshold_is_normal",
            RoundResult {
                round: 10,
                damage_events: vec![dmg("hero", "orc", 4, 10), dmg("rogue", "orc", 4, 10)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
        ),
        (
            "expired_effects_do_not_count_as_dramatic",
            RoundResult {
                round: 11,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec!["poisoned".into()],
            },
            None,
        ),
    ];

    let entries: Vec<Value> = cases
        .iter()
        .map(|(name, round, killed)| {
            let event = classify_round(round, *killed);
            json!({
                "name": *name,
                "input": round_json(round, *killed),
                "expected": combat_event_str(&event),
            })
        })
        .collect();

    json!({"cases": entries})
}

fn classify_combat_outcome_cases() -> Value {
    let cases: Vec<(&str, RoundResult, Option<&str>, Option<f64>)> = vec![
        (
            "killed_yields_killing_blow_top_priority",
            RoundResult {
                round: 1,
                damage_events: vec![dmg("hero", "orc", 100, 1)],
                effects_applied: vec!["bleeding".into()],
                effects_expired: vec![],
            },
            Some("orc"),
            Some(0.0),
        ),
        (
            "near_miss_when_low_hp_and_some_damage",
            RoundResult {
                round: 2,
                damage_events: vec![dmg("hero", "orc", 3, 2)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.15),
        ),
        (
            "near_miss_at_exact_threshold",
            RoundResult {
                round: 3,
                damage_events: vec![dmg("hero", "orc", 3, 3)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.20),
        ),
        (
            "no_near_miss_just_above_threshold",
            RoundResult {
                round: 4,
                damage_events: vec![dmg("hero", "orc", 3, 4)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.21),
        ),
        (
            "near_miss_only_when_some_damage_dealt",
            RoundResult {
                round: 5,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.10),
        ),
        (
            "critical_hit_at_dramatic_damage_threshold",
            RoundResult {
                round: 6,
                damage_events: vec![dmg("hero", "orc", 15, 6)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            None,
        ),
        (
            "critical_hit_high_damage_no_low_hp",
            RoundResult {
                round: 7,
                damage_events: vec![dmg("hero", "orc", 50, 7)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.5),
        ),
        (
            "first_blood_when_only_effects_no_damage",
            RoundResult {
                round: 8,
                damage_events: vec![],
                effects_applied: vec!["dazed".into()],
                effects_expired: vec![],
            },
            None,
            None,
        ),
        (
            "boring_no_damage_no_effects_no_kill",
            RoundResult {
                round: 9,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            None,
        ),
        (
            "normal_some_damage_no_kill_no_effects",
            RoundResult {
                round: 10,
                damage_events: vec![dmg("hero", "orc", 5, 10)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            Some(0.6),
        ),
        (
            "lowest_hp_none_skips_near_miss_check",
            RoundResult {
                round: 11,
                damage_events: vec![dmg("hero", "orc", 3, 11)],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            None,
            None,
        ),
        (
            "killed_empty_string_still_killing_blow",
            RoundResult {
                round: 12,
                damage_events: vec![],
                effects_applied: vec![],
                effects_expired: vec![],
            },
            Some(""),
            None,
        ),
    ];

    let entries: Vec<Value> = cases
        .iter()
        .map(|(name, round, killed, ratio)| {
            let cls = classify_combat_outcome(round, *killed, *ratio);
            json!({
                "name": *name,
                "input": {
                    "round": round_json(round, *killed),
                    "lowest_hp_ratio": ratio,
                },
                "expected": classification_json(&cls),
            })
        })
        .collect();

    json!({"cases": entries})
}

#[derive(Clone, Copy)]
enum Step {
    Observe {
        round_id: u32,
        damage: i32,
        effects: bool,
        killed: Option<&'static str>,
        lowest_hp_ratio: Option<f64>,
    },
    Tick,
    UpdateStakes {
        current_hp: i32,
        max_hp: i32,
    },
}

fn step_json(step: &Step) -> Value {
    match step {
        Step::Observe {
            round_id,
            damage,
            effects,
            killed,
            lowest_hp_ratio,
        } => {
            let damage_events = if *damage != 0 {
                vec![json!({
                    "attacker": "hero",
                    "target": "orc",
                    "damage": damage,
                    "round": round_id,
                })]
            } else {
                vec![]
            };
            let effects_applied: Vec<&str> = if *effects { vec!["dazed"] } else { vec![] };
            json!({
                "kind": "observe",
                "round": {
                    "round": round_id,
                    "damage_events": damage_events,
                    "effects_applied": effects_applied,
                    "effects_expired": [],
                    "killed": killed,
                },
                "lowest_hp_ratio": lowest_hp_ratio,
            })
        }
        Step::Tick => json!({"kind": "tick"}),
        Step::UpdateStakes {
            current_hp,
            max_hp,
        } => json!({
            "kind": "update_stakes",
            "current_hp": current_hp,
            "max_hp": max_hp,
        }),
    }
}

fn run_scenario(name: &str, steps: &[Step], thresholds: &DramaThresholds) -> Value {
    let mut tracker = TensionTracker::new();
    let mut step_outputs: Vec<Value> = Vec::new();

    for step in steps {
        match *step {
            Step::Observe {
                round_id,
                damage,
                effects,
                killed,
                lowest_hp_ratio,
            } => {
                let damage_events = if damage != 0 {
                    vec![dmg("hero", "orc", damage, round_id)]
                } else {
                    vec![]
                };
                let effects_applied: Vec<String> =
                    if effects { vec!["dazed".into()] } else { vec![] };
                let round = RoundResult {
                    round: round_id,
                    damage_events,
                    effects_applied,
                    effects_expired: vec![],
                };
                let cls = tracker.observe(&round, killed, lowest_hp_ratio);
                let hint = tracker.pacing_hint(thresholds);
                step_outputs.push(json!({
                    "step": step_json(step),
                    "after": {
                        "action_tension": tracker.action_tension(),
                        "stakes_tension": tracker.stakes_tension(),
                        "drama_weight": tracker.drama_weight(),
                        "active_spike": tracker.active_spike(),
                        "boring_streak": tracker.boring_streak(),
                        "classification": classification_json(&cls),
                        "pacing_hint": {
                            "drama_weight": hint.drama_weight,
                            "target_sentences": hint.target_sentences,
                            "delivery_mode": delivery_mode_str(&hint.delivery_mode),
                            "escalation_beat": hint.escalation_beat,
                            "narrator_directive": hint.narrator_directive(),
                        },
                    },
                }));
            }
            Step::Tick => {
                tracker.tick();
                let hint = tracker.pacing_hint(thresholds);
                step_outputs.push(json!({
                    "step": step_json(step),
                    "after": {
                        "action_tension": tracker.action_tension(),
                        "stakes_tension": tracker.stakes_tension(),
                        "drama_weight": tracker.drama_weight(),
                        "active_spike": tracker.active_spike(),
                        "boring_streak": tracker.boring_streak(),
                        "classification": Value::Null,
                        "pacing_hint": {
                            "drama_weight": hint.drama_weight,
                            "target_sentences": hint.target_sentences,
                            "delivery_mode": delivery_mode_str(&hint.delivery_mode),
                            "escalation_beat": hint.escalation_beat,
                            "narrator_directive": hint.narrator_directive(),
                        },
                    },
                }));
            }
            Step::UpdateStakes {
                current_hp,
                max_hp,
            } => {
                tracker.update_stakes(current_hp, max_hp);
                let hint = tracker.pacing_hint(thresholds);
                step_outputs.push(json!({
                    "step": step_json(step),
                    "after": {
                        "action_tension": tracker.action_tension(),
                        "stakes_tension": tracker.stakes_tension(),
                        "drama_weight": tracker.drama_weight(),
                        "active_spike": tracker.active_spike(),
                        "boring_streak": tracker.boring_streak(),
                        "classification": Value::Null,
                        "pacing_hint": {
                            "drama_weight": hint.drama_weight,
                            "target_sentences": hint.target_sentences,
                            "delivery_mode": delivery_mode_str(&hint.delivery_mode),
                            "escalation_beat": hint.escalation_beat,
                            "narrator_directive": hint.narrator_directive(),
                        },
                    },
                }));
            }
        }
    }

    json!({
        "name": name,
        "thresholds": {
            "sentence_delivery_min": thresholds.sentence_delivery_min,
            "streaming_delivery_min": thresholds.streaming_delivery_min,
            "render_threshold": thresholds.render_threshold,
            "escalation_streak": thresholds.escalation_streak,
            "ramp_length": thresholds.ramp_length,
        },
        "steps": step_outputs,
    })
}

fn write_file(out_dir: &Path, name: &str, value: &Value) {
    let path = out_dir.join(name);
    let pretty = serde_json::to_string_pretty(value).expect("serialize");
    fs::write(&path, format!("{}\n", pretty))
        .unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));
    eprintln!("wrote {}", path.display());
}

fn main() {
    let out_dir: PathBuf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_output_dir);

    fs::create_dir_all(&out_dir).expect("create fixture dir");

    let defaults = DramaThresholds::default();

    write_file(&out_dir, "classify_round.json", &classify_round_cases());
    write_file(
        &out_dir,
        "classify_combat_outcome.json",
        &classify_combat_outcome_cases(),
    );

    // Scenario A — escalating combat: damage trades climbing toward dramatic
    // damage, then a killing blow. Stakes ramp via update_stakes.
    let escalating = run_scenario(
        "escalating",
        &[
            Step::Observe {
                round_id: 1,
                damage: 4,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.95),
            },
            Step::UpdateStakes {
                current_hp: 90,
                max_hp: 100,
            },
            Step::Observe {
                round_id: 2,
                damage: 8,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.78),
            },
            Step::UpdateStakes {
                current_hp: 70,
                max_hp: 100,
            },
            Step::Observe {
                round_id: 3,
                damage: 18,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.45),
            },
            Step::UpdateStakes {
                current_hp: 40,
                max_hp: 100,
            },
            Step::Observe {
                round_id: 4,
                damage: 22,
                effects: true,
                killed: Some("orc"),
                lowest_hp_ratio: Some(0.0),
            },
        ],
        &defaults,
    );
    write_file(&out_dir, "scenario_escalating.json", &escalating);

    // Scenario B — stalling combat: sequence of boring turns triggers
    // escalation_beat once boring_streak crosses the threshold.
    let stalling = run_scenario(
        "stalling",
        &[
            Step::Observe {
                round_id: 1,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Observe {
                round_id: 2,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Observe {
                round_id: 3,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Observe {
                round_id: 4,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Observe {
                round_id: 5,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Observe {
                round_id: 6,
                damage: 0,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.9),
            },
            Step::Tick,
        ],
        &defaults,
    );
    write_file(&out_dir, "scenario_stalling.json", &stalling);

    // Scenario C — reversal: stakes high (near death), then a dramatic spike,
    // then several quiet ticks while spike decays.
    let reversal = run_scenario(
        "reversal",
        &[
            Step::UpdateStakes {
                current_hp: 10,
                max_hp: 100,
            },
            Step::Observe {
                round_id: 1,
                damage: 30,
                effects: false,
                killed: None,
                lowest_hp_ratio: Some(0.05),
            },
            Step::UpdateStakes {
                current_hp: 75,
                max_hp: 100,
            },
            Step::Tick,
            Step::Tick,
            Step::Tick,
            Step::Tick,
            Step::Tick,
        ],
        &defaults,
    );
    write_file(&out_dir, "scenario_reversal.json", &reversal);

    eprintln!("\nFixtures regenerated. {} cases in classify_round, {} in classify_combat_outcome, 3 scenarios.",
        classify_round_cases()["cases"].as_array().unwrap().len(),
        classify_combat_outcome_cases()["cases"].as_array().unwrap().len(),
    );
}
