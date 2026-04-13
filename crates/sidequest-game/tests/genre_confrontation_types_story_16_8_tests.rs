//! Story 16-8: Genre-specific confrontation types — net combat, ship combat, auction
//!
//! RED phase tests. Verify genre-specific confrontation declarations load from
//! their respective rules.yaml files and have the correct structure.
//!
//! ACs tested:
//!   AC1: neon_dystopia declares net_combat (trace metric ascending, ICE beats, deck secondary stats)
//!   AC2: space_opera declares ship_combat (engagement_range metric, broadside/evasion beats, ship_block secondary stats)
//!   AC3: victoria declares auction (bid metric ascending, raise/bluff/withdraw beats, purse secondary stat)
//!   AC4: Genre loader parses all three confrontation types from YAML
//!   AC5: Each type has correct metric direction, beats with stat checks, and secondary stats
//!   AC6: Confrontation types have mood declarations for music routing

use sidequest_genre::{
    load_rules_config, BeatDef, ConfrontationDef, RulesConfig, SecondaryStatDef,
};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

/// Path to genre packs in sidequest-content (relative to workspace root).
fn genre_pack_path(genre: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // sidequest-api/
        .unwrap()
        .parent() // oq-1/
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs")
        .join(genre)
}

/// Load rules.yaml for a genre pack via the production `load_rules_config`
/// loader so that `_from` pointers (story 38-4) on fields like
/// `interaction_table` are resolved before deserialization.
fn load_rules_yaml(genre: &str) -> RulesConfig {
    let pack = genre_pack_path(genre);
    let path = pack.join("rules.yaml");
    load_rules_config(&path, &pack)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

fn find_confrontation<'a>(rules: &'a RulesConfig, ctype: &str) -> &'a ConfrontationDef {
    rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == ctype)
        .unwrap_or_else(|| panic!("confrontation type '{}' not found in rules", ctype))
}

fn find_beat<'a>(confrontation: &'a ConfrontationDef, beat_id: &str) -> &'a BeatDef {
    confrontation
        .beats
        .iter()
        .find(|b| b.id == beat_id)
        .unwrap_or_else(|| {
            panic!(
                "beat '{}' not found in confrontation '{}'",
                beat_id, confrontation.confrontation_type
            )
        })
}

fn find_secondary_stat<'a>(
    confrontation: &'a ConfrontationDef,
    stat_name: &str,
) -> &'a SecondaryStatDef {
    confrontation
        .secondary_stats
        .iter()
        .find(|s| s.name == stat_name)
        .unwrap_or_else(|| {
            panic!(
                "secondary stat '{}' not found in confrontation '{}'",
                stat_name, confrontation.confrontation_type
            )
        })
}

// ═══════════════════════════════════════════════════════════
// AC1: neon_dystopia net_combat
// ═══════════════════════════════════════════════════════════

#[test]
fn neon_dystopia_has_net_combat_confrontation() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert_eq!(net.confrontation_type, "net_combat");
}

#[test]
fn net_combat_label() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert!(
        !net.label.is_empty(),
        "net_combat should have a display label"
    );
}

#[test]
fn net_combat_category_is_combat() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert_eq!(net.category, "combat");
}

#[test]
fn net_combat_metric_is_trace_ascending() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert_eq!(net.metric.name, "trace");
    assert_eq!(net.metric.direction, "ascending");
}

#[test]
fn net_combat_metric_has_threshold_high() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert!(
        net.metric.threshold_high.is_some(),
        "trace metric should have an upper threshold (detection)"
    );
}

#[test]
fn net_combat_has_ice_beats() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    // ICE encounters as beats — expect at least: breach, bypass_ice, jack_out
    let beat_ids: Vec<&str> = net.beats.iter().map(|b| b.id.as_str()).collect();
    assert!(
        beat_ids.contains(&"breach"),
        "net_combat should have a 'breach' beat"
    );
    assert!(
        beat_ids.contains(&"bypass_ice"),
        "net_combat should have a 'bypass_ice' beat"
    );
    assert!(
        beat_ids.contains(&"jack_out"),
        "net_combat should have a 'jack_out' beat"
    );
}

#[test]
fn net_combat_breach_checks_net_stat() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    let breach = find_beat(net, "breach");
    assert_eq!(breach.stat_check, "Net");
}

#[test]
fn net_combat_bypass_ice_has_risk() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    let bypass = find_beat(net, "bypass_ice");
    assert!(
        bypass.risk.is_some(),
        "bypass_ice should have a risk (trace spike)"
    );
}

#[test]
fn net_combat_jack_out_is_resolution() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    let jack_out = find_beat(net, "jack_out");
    assert_eq!(
        jack_out.resolution,
        Some(true),
        "jack_out should be a resolution beat"
    );
}

#[test]
fn net_combat_has_deck_secondary_stats() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert!(
        !net.secondary_stats.is_empty(),
        "net_combat should have deck secondary stats"
    );
    // Expect at least: programs, firewall
    let stat_names: Vec<&str> = net
        .secondary_stats
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        stat_names.contains(&"programs"),
        "deck should have 'programs' stat"
    );
    assert!(
        stat_names.contains(&"firewall"),
        "deck should have 'firewall' stat"
    );
}

#[test]
fn net_combat_deck_stats_have_source_stat() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    let programs = find_secondary_stat(net, "programs");
    assert!(
        !programs.source_stat.is_empty(),
        "programs should derive from a stat"
    );
    let firewall = find_secondary_stat(net, "firewall");
    assert!(
        !firewall.source_stat.is_empty(),
        "firewall should derive from a stat"
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: space_opera ship_combat
// ═══════════════════════════════════════════════════════════

#[test]
fn space_opera_has_ship_combat_confrontation() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert_eq!(ship.confrontation_type, "ship_combat");
}

#[test]
fn ship_combat_label() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert!(
        !ship.label.is_empty(),
        "ship_combat should have a display label"
    );
}

#[test]
fn ship_combat_category_is_combat() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert_eq!(ship.category, "combat");
}

#[test]
fn ship_combat_metric_is_engagement_range() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert_eq!(ship.metric.name, "engagement_range");
}

#[test]
fn ship_combat_metric_direction() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    // Engagement range can be ascending (closing) or bidirectional
    assert!(
        ship.metric.direction == "ascending" || ship.metric.direction == "bidirectional",
        "engagement_range should be ascending or bidirectional, got '{}'",
        ship.metric.direction
    );
}

#[test]
fn ship_combat_has_broadside_and_evasion_beats() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    let beat_ids: Vec<&str> = ship.beats.iter().map(|b| b.id.as_str()).collect();
    assert!(
        beat_ids.contains(&"broadside"),
        "ship_combat should have a 'broadside' beat"
    );
    assert!(
        beat_ids.contains(&"evasive_maneuver"),
        "ship_combat should have an 'evasive_maneuver' beat"
    );
}

#[test]
fn ship_combat_has_close_range_beat() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    let beat_ids: Vec<&str> = ship.beats.iter().map(|b| b.id.as_str()).collect();
    assert!(
        beat_ids.contains(&"close_range") || beat_ids.contains(&"close_distance"),
        "ship_combat should have a closing beat"
    );
}

#[test]
fn ship_combat_broadside_stat_check() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    let broadside = find_beat(ship, "broadside");
    // Ship combat should use a ship-relevant stat
    assert!(
        !broadside.stat_check.is_empty(),
        "broadside should have a stat_check"
    );
}

#[test]
fn ship_combat_has_ship_block_secondary_stats() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert!(
        !ship.secondary_stats.is_empty(),
        "ship_combat should have ship_block secondary stats"
    );
    // Expect RigStats-pattern: shields, hull, engines, weapons
    let stat_names: Vec<&str> = ship
        .secondary_stats
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        stat_names.contains(&"shields"),
        "ship_block should have 'shields'"
    );
    assert!(
        stat_names.contains(&"hull"),
        "ship_block should have 'hull'"
    );
}

#[test]
fn ship_combat_secondary_stats_have_source_stats() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    let shields = find_secondary_stat(ship, "shields");
    assert!(
        !shields.source_stat.is_empty(),
        "shields should derive from a stat"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: victoria auction
// ═══════════════════════════════════════════════════════════

#[test]
fn victoria_has_auction_confrontation() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert_eq!(auction.confrontation_type, "auction");
}

#[test]
fn auction_label() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert!(
        !auction.label.is_empty(),
        "auction should have a display label"
    );
}

#[test]
fn auction_category_is_social() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert_eq!(auction.category, "social");
}

#[test]
fn auction_metric_is_bid_ascending() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert_eq!(auction.metric.name, "bid");
    assert_eq!(auction.metric.direction, "ascending");
}

#[test]
fn auction_has_raise_bluff_withdraw_beats() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let beat_ids: Vec<&str> = auction.beats.iter().map(|b| b.id.as_str()).collect();
    assert!(
        beat_ids.contains(&"raise"),
        "auction should have a 'raise' beat"
    );
    assert!(
        beat_ids.contains(&"bluff"),
        "auction should have a 'bluff' beat"
    );
    assert!(
        beat_ids.contains(&"withdraw"),
        "auction should have a 'withdraw' beat"
    );
}

#[test]
fn auction_raise_checks_stat() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let raise = find_beat(auction, "raise");
    assert!(
        !raise.stat_check.is_empty(),
        "raise should have a stat_check"
    );
}

#[test]
fn auction_bluff_has_risk() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let bluff = find_beat(auction, "bluff");
    assert!(bluff.risk.is_some(), "bluff should have a risk (exposed)");
}

#[test]
fn auction_withdraw_is_resolution() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let withdraw = find_beat(auction, "withdraw");
    assert_eq!(
        withdraw.resolution,
        Some(true),
        "withdraw should be a resolution beat"
    );
}

#[test]
fn auction_has_purse_secondary_stat() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let stat_names: Vec<&str> = auction
        .secondary_stats
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        stat_names.contains(&"purse"),
        "auction should have 'purse' secondary stat"
    );
}

#[test]
fn auction_purse_is_spendable() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    let purse = find_secondary_stat(auction, "purse");
    assert!(purse.spendable, "purse should be spendable during auction");
}

// ═══════════════════════════════════════════════════════════
// AC4: Genre loader parses all three from YAML
// ═══════════════════════════════════════════════════════════

#[test]
fn neon_dystopia_loads_without_error() {
    let rules = load_rules_yaml("neon_dystopia");
    assert!(
        !rules.confrontations.is_empty(),
        "neon_dystopia should have confrontation declarations"
    );
}

#[test]
fn neon_dystopia_has_both_negotiation_and_net_combat() {
    let rules = load_rules_yaml("neon_dystopia");
    let types: Vec<&str> = rules
        .confrontations
        .iter()
        .map(|c| c.confrontation_type.as_str())
        .collect();
    assert!(
        types.contains(&"negotiation"),
        "should retain existing negotiation"
    );
    assert!(types.contains(&"net_combat"), "should add net_combat");
}

#[test]
fn space_opera_loads_without_error() {
    let rules = load_rules_yaml("space_opera");
    assert!(
        !rules.confrontations.is_empty(),
        "space_opera should have confrontation declarations"
    );
}

#[test]
fn space_opera_has_both_negotiation_and_ship_combat() {
    let rules = load_rules_yaml("space_opera");
    let types: Vec<&str> = rules
        .confrontations
        .iter()
        .map(|c| c.confrontation_type.as_str())
        .collect();
    assert!(
        types.contains(&"negotiation"),
        "should retain existing negotiation"
    );
    assert!(types.contains(&"ship_combat"), "should add ship_combat");
}

#[test]
fn victoria_loads_without_error() {
    let rules = load_rules_yaml("victoria");
    assert!(
        !rules.confrontations.is_empty(),
        "victoria should have confrontation declarations"
    );
}

#[test]
fn victoria_has_negotiation_trial_and_auction() {
    let rules = load_rules_yaml("victoria");
    let types: Vec<&str> = rules
        .confrontations
        .iter()
        .map(|c| c.confrontation_type.as_str())
        .collect();
    assert!(
        types.contains(&"negotiation"),
        "should retain existing negotiation"
    );
    assert!(types.contains(&"trial"), "should retain existing trial");
    assert!(types.contains(&"auction"), "should add auction");
}

// ═══════════════════════════════════════════════════════════
// AC5: Correct metric direction, beats with stat checks, secondary stats
// ═══════════════════════════════════════════════════════════

#[test]
fn net_combat_all_beats_have_stat_checks() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    for beat in &net.beats {
        assert!(
            !beat.stat_check.is_empty(),
            "beat '{}' in net_combat should have a stat_check",
            beat.id
        );
    }
}

#[test]
fn ship_combat_all_beats_have_stat_checks() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    for beat in &ship.beats {
        assert!(
            !beat.stat_check.is_empty(),
            "beat '{}' in ship_combat should have a stat_check",
            beat.id
        );
    }
}

#[test]
fn auction_all_beats_have_stat_checks() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    for beat in &auction.beats {
        assert!(
            !beat.stat_check.is_empty(),
            "beat '{}' in auction should have a stat_check",
            beat.id
        );
    }
}

#[test]
fn net_combat_metric_starting_is_zero() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert_eq!(net.metric.starting, 0, "trace starts at 0 (undetected)");
}

#[test]
fn auction_metric_has_threshold_high() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert!(
        auction.metric.threshold_high.is_some(),
        "bid metric should have an upper threshold (winning bid)"
    );
}

// ═══════════════════════════════════════════════════════════
// AC6: Mood declarations for music routing
// ═══════════════════════════════════════════════════════════

#[test]
fn net_combat_has_mood() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    assert!(
        net.mood.is_some(),
        "net_combat should declare a mood for music routing"
    );
}

#[test]
fn ship_combat_has_mood() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert!(
        ship.mood.is_some(),
        "ship_combat should declare a mood for music routing"
    );
}

#[test]
fn auction_has_mood() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert!(
        auction.mood.is_some(),
        "auction should declare a mood for music routing"
    );
}

#[test]
fn net_combat_mood_is_tension_or_combat() {
    let rules = load_rules_yaml("neon_dystopia");
    let net = find_confrontation(&rules, "net_combat");
    let mood = net.mood.as_deref().unwrap();
    assert!(
        mood == "tension" || mood == "combat",
        "net_combat mood should be 'tension' or 'combat', got '{}'",
        mood
    );
}

#[test]
fn ship_combat_mood_is_combat() {
    let rules = load_rules_yaml("space_opera");
    let ship = find_confrontation(&rules, "ship_combat");
    assert_eq!(
        ship.mood.as_deref(),
        Some("combat"),
        "ship_combat mood should be 'combat'"
    );
}

#[test]
fn auction_mood_is_tension() {
    let rules = load_rules_yaml("victoria");
    let auction = find_confrontation(&rules, "auction");
    assert_eq!(
        auction.mood.as_deref(),
        Some("tension"),
        "auction mood should be 'tension'"
    );
}
