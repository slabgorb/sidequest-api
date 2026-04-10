//! Story 16-6: Standoff confrontation type — spaghetti_western pre-combat encounter
//!
//! RED phase — Standoff is the first genre-specific confrontation type. It exercises
//! the full pipeline: YAML declaration → ConfrontationDef parsing → StructuredEncounter
//! instantiation → beat dispatch → metric mutation → resolution → combat escalation.
//!
//! Key mappings:
//!   - tension → EncounterMetric (name="tension", Ascending, threshold_high=10)
//!   - beats (size_up, bluff, flinch, draw) → metric deltas (+2, +3, -1, resolve)
//!   - focus → SecondaryStats (sourced from NERVE, spendable)
//!   - escalates_to: combat → CombatState transition
//!   - mood: standoff → mood_override on encounter
//!
//! ACs:
//!   AC-Loads:     Standoff declaration parses from spaghetti_western rules.yaml
//!   AC-Beats:     Size Up, Bluff, Flinch, Draw all modify tension correctly
//!   AC-Reveals:   Size Up reveals one opponent detail per beat
//!   AC-Resolution: Draw resolves with DRAW check + tension modifier
//!   AC-Escalation: Resolved standoff transitions to CombatState with initiative
//!   AC-Mood:      MusicDirector plays "standoff" mood during encounter
//!   AC-Context:   Full standoff context injected into narrator prompt
//!   AC-Integration: Complete standoff sequence from start to combat escalation

use sidequest_game::encounter::{
    EncounterActor, EncounterPhase, MetricDirection,
    StructuredEncounter,
};
use sidequest_genre::{BeatDef, ConfrontationDef};

// =========================================================================
// AC-Loads: Standoff declaration parses from spaghetti_western rules.yaml
// =========================================================================

/// Helper to locate genre packs directory.
fn genre_packs_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("GENRE_PACKS_PATH") {
        return std::path::PathBuf::from(path);
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs")
}

/// spaghetti_western rules.yaml must declare a standoff confrontation.
#[test]
fn spaghetti_western_has_standoff_confrontation() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff");

    assert!(
        standoff.is_some(),
        "spaghetti_western must declare a 'standoff' confrontation type"
    );
}

/// Standoff confrontation has correct category and metric.
#[test]
fn standoff_has_correct_schema() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    assert_eq!(standoff.category, "pre_combat", "standoff is pre-combat");
    assert_eq!(standoff.metric.name, "tension", "metric tracks tension");
    assert_eq!(standoff.metric.direction, "ascending", "tension builds up");
    assert_eq!(standoff.metric.starting, 0, "tension starts at zero");
    assert_eq!(
        standoff.metric.threshold_high,
        Some(10),
        "standoff resolves at tension 10"
    );
}

/// Standoff has all four beats: size_up, bluff, flinch, draw.
#[test]
fn standoff_has_four_beats() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    let beat_ids: Vec<&str> = standoff.beats.iter().map(|b| b.id.as_str()).collect();

    assert!(beat_ids.contains(&"size_up"), "must have size_up beat");
    assert!(beat_ids.contains(&"bluff"), "must have bluff beat");
    assert!(beat_ids.contains(&"flinch"), "must have flinch beat");
    assert!(beat_ids.contains(&"draw"), "must have draw beat");
    assert_eq!(standoff.beats.len(), 4, "exactly 4 beats in standoff");
}

/// Standoff beats have correct metric deltas.
#[test]
fn standoff_beat_deltas_correct() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    let find_beat = |id: &str| -> &BeatDef {
        standoff.beats.iter().find(|b| b.id == id).unwrap()
    };

    assert_eq!(find_beat("size_up").metric_delta, 2, "size_up: tension +2");
    assert_eq!(find_beat("bluff").metric_delta, 3, "bluff: tension +3");
    assert_eq!(find_beat("flinch").metric_delta, -1, "flinch: tension -1");
    // draw's delta is 0 — it resolves via stat check, not metric push
    assert_eq!(find_beat("draw").metric_delta, 0, "draw: delta 0 (resolves)");
}

/// Size Up beat reveals opponent detail.
#[test]
fn standoff_size_up_reveals_opponent_detail() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    let size_up = standoff.beats.iter().find(|b| b.id == "size_up").unwrap();
    assert_eq!(
        size_up.reveals.as_deref(),
        Some("opponent_detail"),
        "size_up must reveal opponent_detail"
    );
    assert_eq!(
        size_up.stat_check, "CUNNING",
        "size_up checks CUNNING"
    );
}

/// Draw beat is the resolution beat.
#[test]
fn standoff_draw_is_resolution_beat() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    let draw = standoff.beats.iter().find(|b| b.id == "draw").unwrap();
    assert!(
        draw.resolution.unwrap_or(false),
        "draw must be a resolution beat"
    );
    assert_eq!(draw.stat_check, "DRAW", "draw checks the DRAW stat");
}

/// Bluff beat has risk description.
#[test]
fn standoff_bluff_has_risk() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    let bluff = standoff.beats.iter().find(|b| b.id == "bluff").unwrap();
    assert!(
        bluff.risk.is_some(),
        "bluff must have a risk description"
    );
    assert_eq!(bluff.stat_check, "NERVE", "bluff checks NERVE");
}

/// Standoff has focus secondary stat sourced from NERVE.
#[test]
fn standoff_has_focus_secondary_stat() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    assert_eq!(standoff.secondary_stats.len(), 1, "one secondary stat: focus");
    assert_eq!(standoff.secondary_stats[0].name, "focus");
    assert_eq!(
        standoff.secondary_stats[0].source_stat, "NERVE",
        "focus sourced from NERVE"
    );
    assert!(standoff.secondary_stats[0].spendable, "focus is spendable");
}

/// Standoff escalates to combat.
#[test]
fn standoff_escalates_to_combat() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    assert_eq!(
        standoff.escalates_to.as_deref(),
        Some("combat"),
        "standoff must escalate to combat"
    );
}

/// Standoff has mood override.
#[test]
fn standoff_mood_is_standoff() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist");

    assert_eq!(
        standoff.mood.as_deref(),
        Some("standoff"),
        "standoff mood must be 'standoff'"
    );
}

/// spaghetti_western still validates after adding confrontations.
#[test]
fn spaghetti_western_validates_with_standoff() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let result = pack.validate();
    assert!(
        result.is_ok(),
        "spaghetti_western must validate with standoff confrontation: {:?}",
        result.err()
    );
}

// =========================================================================
// AC-Beats / AC-Resolution: StructuredEncounter::from_confrontation_def()
// and apply_beat() for metric mutation
// =========================================================================

/// Helper: build a standoff ConfrontationDef inline for unit tests.
fn standoff_def() -> ConfrontationDef {
    let yaml = r#"
type: standoff
label: "Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: size_up
    label: "Size Up"
    metric_delta: 2
    stat_check: CUNNING
    reveals: opponent_detail
  - id: bluff
    label: "Bluff"
    metric_delta: 3
    stat_check: NERVE
    risk: "opponent may call it — immediate draw"
  - id: flinch
    label: "Flinch"
    metric_delta: -1
    stat_check: NERVE
  - id: draw
    label: "Draw"
    metric_delta: 0
    stat_check: DRAW
    resolution: true
secondary_stats:
  - name: focus
    source_stat: NERVE
    spendable: true
escalates_to: combat
mood: standoff
"#;
    serde_yaml::from_str(yaml).expect("standoff def should parse")
}

/// from_confrontation_def() creates a standoff encounter with correct initial state.
#[test]
fn from_confrontation_def_creates_standoff() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.encounter_type, "standoff");
    assert_eq!(encounter.metric.name, "tension");
    assert_eq!(encounter.metric.current, 0, "tension starts at 0");
    assert_eq!(encounter.metric.starting, 0);
    assert_eq!(encounter.metric.direction, MetricDirection::Ascending);
    assert_eq!(encounter.metric.threshold_high, Some(10));
    assert!(!encounter.resolved);
    assert_eq!(encounter.beat, 0);
    assert_eq!(
        encounter.mood_override.as_deref(),
        Some("standoff"),
        "mood_override must be set from confrontation def"
    );
}

/// from_confrontation_def() populates secondary stats.
#[test]
fn from_confrontation_def_populates_focus_stat() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    let stats = encounter
        .secondary_stats
        .as_ref()
        .expect("standoff must have secondary stats");
    let focus = stats.stats.get("focus").expect("must have focus stat");
    assert!(focus.max > 0, "focus max should be positive");
    assert_eq!(focus.current, focus.max, "focus starts at max");
}

/// from_confrontation_def() starts in Setup phase.
#[test]
fn from_confrontation_def_starts_in_setup() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(
        encounter.structured_phase,
        Some(EncounterPhase::Setup),
        "standoff starts in Setup phase"
    );
}

/// apply_beat("size_up") increases tension by 2.
#[test]
fn apply_beat_size_up_increases_tension() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let result = encounter.apply_beat("size_up", &def);
    assert!(result.is_ok(), "size_up should succeed");

    assert_eq!(
        encounter.metric.current, 2,
        "tension should be 2 after size_up (+2)"
    );
    assert!(!encounter.resolved, "not resolved at tension 2");
}

/// apply_beat("bluff") increases tension by 3.
#[test]
fn apply_beat_bluff_increases_tension() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _result = encounter.apply_beat("bluff", &def);
    assert_eq!(
        encounter.metric.current, 3,
        "tension should be 3 after bluff (+3)"
    );
}

/// apply_beat("flinch") decreases tension by 1.
#[test]
fn apply_beat_flinch_decreases_tension() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // First build some tension
    let _ = encounter.apply_beat("size_up", &def); // tension = 2
    let _ = encounter.apply_beat("flinch", &def);  // tension = 1

    assert_eq!(
        encounter.metric.current, 1,
        "tension should be 1 after size_up(+2) then flinch(-1)"
    );
}

/// Tension cannot go below 0.
#[test]
fn tension_does_not_go_negative() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Flinch at tension 0 should clamp to 0, not go to -1
    let _ = encounter.apply_beat("flinch", &def);
    assert!(
        encounter.metric.current >= 0,
        "tension should not go below 0, got {}",
        encounter.metric.current
    );
}

/// apply_beat with unknown beat_id returns error.
#[test]
fn apply_beat_unknown_id_returns_error() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let result = encounter.apply_beat("nonexistent", &def);
    assert!(
        result.is_err(),
        "unknown beat id should return error, not silently succeed"
    );
}

/// Multiple size_up beats accumulate tension.
#[test]
fn multiple_beats_accumulate_tension() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def); // 2
    let _ = encounter.apply_beat("size_up", &def); // 4
    let _ = encounter.apply_beat("bluff", &def);   // 7

    assert_eq!(encounter.metric.current, 7, "tension should accumulate: 2+2+3=7");
    assert_eq!(encounter.beat, 3, "beat counter should be 3");
}

// =========================================================================
// AC-Resolution: Tension reaching threshold resolves the encounter
// =========================================================================

/// When tension reaches threshold_high (10), encounter resolves.
#[test]
fn tension_threshold_resolves_encounter() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Build tension to exactly 10: size_up(2) + size_up(2) + bluff(3) + bluff(3) = 10
    let _ = encounter.apply_beat("size_up", &def); // 2
    let _ = encounter.apply_beat("size_up", &def); // 4
    let _ = encounter.apply_beat("bluff", &def);   // 7
    let _ = encounter.apply_beat("bluff", &def);   // 10

    assert_eq!(encounter.metric.current, 10);
    assert!(
        encounter.resolved,
        "encounter must resolve when tension reaches threshold (10)"
    );
}

/// Draw beat resolves the encounter regardless of tension level.
#[test]
fn draw_beat_resolves_encounter() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Tension at 2, well below threshold — but draw is a resolution beat
    let _ = encounter.apply_beat("size_up", &def); // 2
    let result = encounter.apply_beat("draw", &def);

    assert!(result.is_ok());
    assert!(
        encounter.resolved,
        "draw beat must resolve the encounter even below threshold"
    );
}

/// After resolution, further beats are rejected.
#[test]
fn beats_rejected_after_resolution() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("draw", &def); // resolve
    assert!(encounter.resolved);

    let result = encounter.apply_beat("size_up", &def);
    assert!(
        result.is_err(),
        "beats should be rejected after encounter is resolved"
    );
}

// =========================================================================
// AC-Escalation: Resolved standoff transitions to CombatState
// =========================================================================

/// escalation_target() returns "combat" for standoff encounters.
#[test]
fn standoff_escalation_target_is_combat() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(
        encounter.escalation_target(&def).as_deref(),
        Some("combat"),
        "standoff must escalate to combat"
    );
}

/// After resolution, encounter can produce a CombatState for escalation.
#[test]
fn resolved_standoff_produces_combat_escalation() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Add actors (two duelists)
    encounter.actors = vec![
        EncounterActor {
            name: "The Man with No Name".to_string(),
            role: "duelist".to_string(),
        },
        EncounterActor {
            name: "Angel Eyes".to_string(),
            role: "duelist".to_string(),
        },
    ];

    // Build tension and resolve via draw
    let _ = encounter.apply_beat("size_up", &def); // 2
    let _ = encounter.apply_beat("bluff", &def);   // 5
    let _ = encounter.apply_beat("draw", &def);    // resolve

    assert!(encounter.resolved);

    // Escalation should produce a combat encounter
    let combat = encounter.escalate_to_combat();
    assert!(
        combat.is_some(),
        "resolved standoff with escalates_to=combat must produce CombatState"
    );

    let combat = combat.unwrap();
    assert_eq!(
        combat.encounter_type, "combat",
        "escalated encounter type must be combat"
    );
    assert_eq!(
        combat.actors.len(),
        2,
        "combat actors should be carried from standoff"
    );
}

// =========================================================================
// AC-Mood: MusicDirector plays "standoff" mood
// =========================================================================

/// Standoff encounter has mood_override set to "standoff".
#[test]
fn standoff_encounter_mood_override() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(
        encounter.mood_override.as_deref(),
        Some("standoff"),
        "standoff encounter must have mood_override='standoff'"
    );
}

// =========================================================================
// AC-Context: Narrator prompt context for standoff
// =========================================================================

/// format_encounter_context produces a complete standoff context block.
#[test]
fn format_standoff_context_includes_all_sections() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Simulate a mid-standoff state: 3 beats in, tension at 7
    let _ = encounter.apply_beat("size_up", &def); // 2
    let _ = encounter.apply_beat("bluff", &def);   // 5
    let _ = encounter.apply_beat("size_up", &def); // 7

    let ctx = encounter.format_encounter_context(&def);

    assert!(ctx.contains("[STANDOFF]"), "context must have [STANDOFF] header");
    assert!(ctx.contains("tension") || ctx.contains("Tension"), "must show tension metric");
    assert!(ctx.contains("7"), "must show current tension value (7)");
    assert!(ctx.contains("10"), "must show threshold (10)");
    assert!(ctx.contains("Size Up"), "must list available beats");
    assert!(ctx.contains("Bluff"), "must list bluff beat");
    assert!(ctx.contains("Draw"), "must list draw beat");
    assert!(ctx.contains("Flinch"), "must list flinch beat");
}

/// Narrator context includes focus secondary stat.
#[test]
fn format_standoff_context_includes_focus() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    let ctx = encounter.format_encounter_context(&def);

    assert!(
        ctx.contains("focus") || ctx.contains("Focus"),
        "context must include focus secondary stat"
    );
}

/// Narrator context includes cinematography hints.
#[test]
fn format_standoff_context_includes_cinematography() {
    let def = standoff_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    let ctx = encounter.format_encounter_context(&def);

    // Standoff spec says: camera_override: close_up_slow_motion, sentence_range: [2, 4]
    assert!(
        ctx.contains("Close") || ctx.contains("close") || ctx.contains("slow"),
        "context must include cinematography hints for standoff"
    );
}

// =========================================================================
// AC-Integration: Full standoff sequence from start to combat escalation
// =========================================================================

/// Complete standoff integration test: setup → beats → draw → combat.
#[test]
fn full_standoff_sequence_to_combat_escalation() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("spaghetti_western"))
        .expect("spaghetti_western should load");

    let standoff_def = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("standoff must exist in spaghetti_western");

    // 1. Create encounter from definition
    let mut encounter = StructuredEncounter::from_confrontation_def(standoff_def);

    assert_eq!(encounter.encounter_type, "standoff");
    assert_eq!(encounter.metric.current, 0);
    assert!(!encounter.resolved);

    // 2. Add duelists
    encounter.actors = vec![
        EncounterActor {
            name: "Blondie".to_string(),
            role: "duelist".to_string(),
        },
        EncounterActor {
            name: "Tuco".to_string(),
            role: "duelist".to_string(),
        },
        EncounterActor {
            name: "Angel Eyes".to_string(),
            role: "duelist".to_string(),
        },
    ];

    // 3. Beat sequence: size_up → bluff → size_up → draw
    let r1 = encounter.apply_beat("size_up", standoff_def);
    assert!(r1.is_ok(), "size_up should succeed");
    assert_eq!(encounter.metric.current, 2, "tension at 2");
    assert_eq!(encounter.beat, 1);

    let r2 = encounter.apply_beat("bluff", standoff_def);
    assert!(r2.is_ok(), "bluff should succeed");
    assert_eq!(encounter.metric.current, 5, "tension at 5");
    assert_eq!(encounter.beat, 2);

    let r3 = encounter.apply_beat("size_up", standoff_def);
    assert!(r3.is_ok(), "second size_up should succeed");
    assert_eq!(encounter.metric.current, 7, "tension at 7");
    assert_eq!(encounter.beat, 3);

    // 4. Draw resolves the standoff
    let r4 = encounter.apply_beat("draw", standoff_def);
    assert!(r4.is_ok(), "draw should succeed");
    assert!(encounter.resolved, "encounter must be resolved after draw");
    assert_eq!(encounter.beat, 4);

    // 5. Verify mood was set throughout
    assert_eq!(encounter.mood_override.as_deref(), Some("standoff"));

    // 6. Escalate to combat
    let combat = encounter.escalate_to_combat();
    assert!(combat.is_some(), "must produce combat encounter");
    let combat = combat.unwrap();
    assert_eq!(combat.encounter_type, "combat");
    assert_eq!(
        combat.actors.len(),
        3,
        "all three duelists should carry to combat"
    );

    // 7. Combat actors preserve names from standoff
    let combat_names: Vec<&str> = combat.actors.iter().map(|a| a.name.as_str()).collect();
    assert!(combat_names.contains(&"Blondie"));
    assert!(combat_names.contains(&"Tuco"));
    assert!(combat_names.contains(&"Angel Eyes"));

    // 8. Narrator context should have been available before resolution
    // (smoke test — the encounter type is correct)
    assert_eq!(combat.structured_phase, Some(EncounterPhase::Setup));
}

// =========================================================================
// AC-Integration: GameSnapshot carries standoff encounter
// =========================================================================

/// GameSnapshot accepts a standoff-type StructuredEncounter.
#[test]
fn game_snapshot_accepts_standoff_encounter() {
    use sidequest_game::state::GameSnapshot;

    let def = standoff_def();
    let mut snapshot = GameSnapshot::default();
    snapshot.encounter = Some(StructuredEncounter::from_confrontation_def(&def));

    let enc = snapshot.encounter.as_ref().expect("encounter set");
    assert_eq!(enc.encounter_type, "standoff");
    assert_eq!(enc.metric.name, "tension");
    assert_eq!(enc.mood_override.as_deref(), Some("standoff"));
}

/// GameSnapshot with standoff encounter survives serde roundtrip.
#[test]
fn game_snapshot_standoff_serde_roundtrip() {
    use sidequest_game::state::GameSnapshot;

    let def = standoff_def();
    let mut snapshot = GameSnapshot::default();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);
    encounter.actors = vec![
        EncounterActor {
            name: "Clint".to_string(),
            role: "duelist".to_string(),
        },
    ];
    snapshot.encounter = Some(encounter);

    let json = serde_json::to_string(&snapshot).expect("serialize");
    let de: GameSnapshot = serde_json::from_str(&json).expect("deserialize");

    let enc = de.encounter.as_ref().expect("encounter survived roundtrip");
    assert_eq!(enc.encounter_type, "standoff");
    assert_eq!(enc.metric.name, "tension");
    assert_eq!(enc.actors.len(), 1);
    assert_eq!(enc.actors[0].name, "Clint");
    assert_eq!(enc.mood_override.as_deref(), Some("standoff"));
}

// =========================================================================
// Phase transitions during standoff
// =========================================================================

/// Phase advances through the dramatic arc as beats accumulate.
#[test]
fn standoff_phase_advances_with_beats() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Setup at beat 0
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Setup));

    // After first beat → Opening
    let _ = encounter.apply_beat("size_up", &def);
    assert_eq!(
        encounter.structured_phase,
        Some(EncounterPhase::Opening),
        "should transition to Opening after first beat"
    );

    // After more beats → Escalation
    let _ = encounter.apply_beat("bluff", &def);
    let _ = encounter.apply_beat("size_up", &def);
    assert!(
        matches!(
            encounter.structured_phase,
            Some(EncounterPhase::Escalation) | Some(EncounterPhase::Climax)
        ),
        "should be in Escalation or Climax by beat 3"
    );
}

/// Resolution sets phase to Resolution.
#[test]
fn standoff_draw_sets_resolution_phase() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("draw", &def);

    assert_eq!(
        encounter.structured_phase,
        Some(EncounterPhase::Resolution),
        "draw should set phase to Resolution"
    );
}

// =========================================================================
// Edge cases
// =========================================================================

/// Flinch from zero tension should not underflow.
#[test]
fn flinch_at_zero_tension_safe() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.current, 0);
    let _ = encounter.apply_beat("flinch", &def);

    // Should clamp to 0, not wrap to i32::MAX or go negative
    assert_eq!(
        encounter.metric.current, 0,
        "flinch at zero should clamp, not underflow"
    );
}

/// Tension overshooting threshold still resolves.
#[test]
fn tension_overshoot_resolves() {
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Push tension past threshold: bluff(3) * 4 = 12, past threshold of 10
    let _ = encounter.apply_beat("bluff", &def); // 3
    let _ = encounter.apply_beat("bluff", &def); // 6
    let _ = encounter.apply_beat("bluff", &def); // 9
    let _ = encounter.apply_beat("bluff", &def); // 12

    assert!(encounter.metric.current >= 10);
    assert!(
        encounter.resolved,
        "encounter must resolve when tension exceeds threshold"
    );
}

// =========================================================================
// Wiring test: StructuredEncounter methods are accessible from lib.rs
// =========================================================================

/// Verify the new methods are re-exported from sidequest_game.
/// This is a compile-time wiring test — if it compiles, the exports exist.
#[test]
fn encounter_methods_exported_from_crate() {
    // from_confrontation_def must be callable via the public API
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);
    let _ = encounter.apply_beat("size_up", &def);
    let _ = encounter.escalation_target(&def);
    let _ = encounter.escalate_to_combat();
    let _ = encounter.format_encounter_context(&def);

    // If this test compiles, the methods are properly exported
    assert!(true);
}
