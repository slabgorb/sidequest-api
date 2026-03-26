//! Game state composition — GameSnapshot and typed patches.
//!
//! Port lesson #4: GameSnapshot composes domain structs, no god object.
//! Each domain struct owns its mutations via typed patch application.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::character::Character;
use crate::chase::{ChaseState, ChaseType};
use crate::combat::CombatState;
use crate::combatant::Combatant;
use crate::creature_core::CreatureCore;
use crate::delta::StateDelta;
use crate::disposition::Disposition;
use crate::inventory::Inventory;
use crate::narrative::NarrativeEntry;
use crate::npc::Npc;
use crate::turn::TurnManager;

use sidequest_protocol::{
    ChapterMarkerPayload, CombatEnemy, CombatEventPayload, ExploredLocation, GameMessage,
    MapUpdatePayload, PartyMember, PartyStatusPayload,
};

/// The complete game state at a point in time.
///
/// Composes all domain types (port lesson #4). Serializable for persistence
/// and WebSocket broadcast. Port lesson #11: captures ALL client-visible fields,
/// not just characters/location/quest_log like the Python version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSnapshot {
    /// Genre pack identifier (e.g., "mutant_wasteland").
    pub genre_slug: String,
    /// World identifier within the genre pack.
    pub world_slug: String,
    /// Player characters.
    pub characters: Vec<Character>,
    /// Non-player characters.
    pub npcs: Vec<Npc>,
    /// Current location name.
    pub location: String,
    /// Current time of day.
    pub time_of_day: String,
    /// Active quests (quest_name → description).
    pub quest_log: HashMap<String, String>,
    /// Player notes.
    pub notes: Vec<String>,
    /// Narrative history.
    pub narrative_log: Vec<NarrativeEntry>,
    /// Active combat state.
    pub combat: CombatState,
    /// Active chase sequence (None if no chase in progress).
    pub chase: Option<ChaseState>,
    /// Currently active narrative tropes.
    pub active_tropes: Vec<String>,
    /// Current atmosphere description.
    pub atmosphere: String,
    /// Current region name.
    pub current_region: String,
    /// Regions the player has visited.
    pub discovered_regions: Vec<String>,
    /// Routes the player has discovered.
    pub discovered_routes: Vec<String>,
    /// Turn sequencing and barrier tracking.
    pub turn_manager: TurnManager,
    /// When this snapshot was last persisted (set by GameStore on save).
    pub last_saved_at: Option<DateTime<Utc>>,
    /// Active narrative stakes description (story 2-7).
    #[serde(default)]
    pub active_stakes: String,
    /// Established lore fragments (story 2-7).
    #[serde(default)]
    pub lore_established: Vec<String>,
}

impl GameSnapshot {
    /// Find a mutable character or NPC by name and apply an HP delta.
    fn apply_hp_change(&mut self, name: &str, delta: i32) {
        for c in &mut self.characters {
            if c.name() == name {
                c.apply_hp_delta(delta);
                return;
            }
        }
        for n in &mut self.npcs {
            if n.name() == name {
                n.apply_hp_delta(delta);
                return;
            }
        }
    }

    /// Apply a world state patch (location, atmosphere, quest_log, etc.).
    /// Only fields that are `Some` in the patch are updated.
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_world_patch(&mut self, patch: &WorldStatePatch) {
        let span = tracing::info_span!(
            "apply_world_patch",
            patch_type = "world",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if let Some(ref loc) = patch.location {
            self.location = loc.clone();
            changed.push("location");
        }
        if let Some(ref tod) = patch.time_of_day {
            self.time_of_day = tod.clone();
            changed.push("time_of_day");
        }
        if let Some(ref atm) = patch.atmosphere {
            self.atmosphere = atm.clone();
            changed.push("atmosphere");
        }
        if let Some(ref ql) = patch.quest_log {
            self.quest_log = ql.clone();
            changed.push("quest_log");
        }
        if let Some(ref updates) = patch.quest_updates {
            for (k, v) in updates {
                self.quest_log.insert(k.clone(), v.clone());
            }
            changed.push("quest_updates");
        }
        if let Some(ref n) = patch.notes {
            self.notes = n.clone();
            changed.push("notes");
        }
        if let Some(ref cr) = patch.current_region {
            self.current_region = cr.clone();
            changed.push("current_region");
        }
        if let Some(ref regions) = patch.discovered_regions {
            self.discovered_regions = regions.clone();
            changed.push("discovered_regions");
        }
        if let Some(ref routes) = patch.discovered_routes {
            self.discovered_routes = routes.clone();
            changed.push("discovered_routes");
        }

        // Append + dedup semantics for discovery
        if let Some(ref regions) = patch.discover_regions {
            for r in regions {
                if !self.discovered_regions.contains(r) {
                    self.discovered_regions.push(r.clone());
                }
            }
            changed.push("discover_regions");
        }
        if let Some(ref routes) = patch.discover_routes {
            for r in routes {
                if !self.discovered_routes.contains(r) {
                    self.discovered_routes.push(r.clone());
                }
            }
            changed.push("discover_routes");
        }

        if let Some(ref stakes) = patch.active_stakes {
            self.active_stakes = stakes.clone();
            changed.push("active_stakes");
        }
        if let Some(ref lore) = patch.lore_established {
            self.lore_established.extend(lore.iter().cloned());
            changed.push("lore_established");
        }

        // HP changes
        if let Some(ref hp) = patch.hp_changes {
            for (name, delta) in hp {
                self.apply_hp_change(name, *delta);
            }
            changed.push("hp_changes");
        }

        // NPC attitude changes
        if let Some(ref attitudes) = patch.npc_attitudes {
            for (name, attitude_str) in attitudes {
                if let Some(disposition) = Disposition::from_attitude_str(attitude_str) {
                    for npc in &mut self.npcs {
                        if npc.name() == name {
                            npc.disposition = disposition;
                        }
                    }
                }
            }
            changed.push("npc_attitudes");
        }

        // NPC upsert
        if let Some(ref npc_patches) = patch.npcs_present {
            for npc_patch in npc_patches {
                let existing = self.npcs.iter_mut().find(|n| n.name() == npc_patch.name);
                if let Some(npc) = existing {
                    npc.merge_patch(npc_patch);
                } else {
                    // Create new NPC from patch
                    let new_npc = Npc {
                        core: CreatureCore {
                            name: NonBlankString::new(&npc_patch.name)
                                .unwrap_or_else(|_| NonBlankString::new("Unknown").unwrap()),
                            description: npc_patch
                                .description
                                .as_ref()
                                .and_then(|d| NonBlankString::new(d).ok())
                                .unwrap_or_else(|| NonBlankString::new("No description").unwrap()),
                            personality: npc_patch
                                .personality
                                .as_ref()
                                .and_then(|p| NonBlankString::new(p).ok())
                                .unwrap_or_else(|| NonBlankString::new("Unknown").unwrap()),
                            level: 1,
                            hp: 10,
                            max_hp: 10,
                            ac: 10,
                            statuses: vec![],
                            inventory: Inventory::default(),
                        },
                        voice_id: None,
                        disposition: Disposition::new(0),
                        location: npc_patch
                            .location
                            .as_ref()
                            .and_then(|l| NonBlankString::new(l).ok()),
                        pronouns: npc_patch.pronouns.clone(),
                        appearance: npc_patch.appearance.clone(),
                    };
                    self.npcs.push(new_npc);
                }
            }
            changed.push("npcs_present");
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }

    /// Apply a combat patch.
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_combat_patch(&mut self, patch: &CombatPatch) {
        let span = tracing::info_span!(
            "apply_combat_patch",
            patch_type = "combat",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if patch.advance_round {
            self.combat.advance_round();
            changed.push("round");
        }
        if let Some(active) = patch.in_combat {
            self.combat.set_in_combat(active);
            changed.push("in_combat");
        }
        if let Some(ref order) = patch.turn_order {
            self.combat.set_turn_order(order.clone());
            changed.push("turn_order");
        }
        if let Some(ref turn) = patch.current_turn {
            self.combat.set_current_turn(turn.clone());
            changed.push("current_turn");
        }
        if let Some(ref actions) = patch.available_actions {
            self.combat.set_available_actions(actions.clone());
            changed.push("available_actions");
        }
        if let Some(weight) = patch.drama_weight {
            self.combat.set_drama_weight(weight);
            changed.push("drama_weight");
        }
        if let Some(ref hp) = patch.hp_changes {
            for (name, delta) in hp {
                self.apply_hp_change(name, *delta);
            }
            changed.push("hp_changes");
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }

    /// Apply a chase patch (start a chase or record a roll).
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_chase_patch(&mut self, patch: &ChasePatch) {
        let span = tracing::info_span!(
            "apply_chase_patch",
            patch_type = "chase",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if let Some((chase_type, threshold)) = patch.start {
            self.chase = Some(ChaseState::new(chase_type, threshold));
            changed.push("chase_started");
        }
        if let Some(roll) = patch.roll {
            if let Some(ref mut chase) = self.chase {
                chase.record_roll(roll);
                changed.push("escape_roll");
            }
        }
        if let Some(sep) = patch.separation {
            if let Some(ref mut chase) = self.chase {
                chase.set_separation(sep);
                changed.push("separation");
            }
        }
        if let Some(ref phase) = patch.phase {
            if let Some(ref mut chase) = self.chase {
                chase.set_phase(phase.clone());
                changed.push("phase");
            }
        }
        if let Some(ref event) = patch.event {
            if let Some(ref mut chase) = self.chase {
                chase.set_event(event.clone());
                changed.push("event");
            }
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }
}

/// Patch for world-level state (location, atmosphere, quests, regions).
///
/// Only `Some` fields are applied; `None` means "no change."
/// Story 2-7: Extended with hp_changes, npc_attitudes, time_of_day, quest merge, region dedup.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldStatePatch {
    /// New location.
    pub location: Option<String>,
    /// New time of day.
    pub time_of_day: Option<String>,
    /// New atmosphere.
    pub atmosphere: Option<String>,
    /// Replacement quest log (full replace, not merge).
    pub quest_log: Option<HashMap<String, String>>,
    /// Quest updates — merged by key (additive).
    pub quest_updates: Option<HashMap<String, String>>,
    /// Replacement notes list.
    pub notes: Option<Vec<String>>,
    /// New current region.
    pub current_region: Option<String>,
    /// Replacement discovered regions list (full replace).
    pub discovered_regions: Option<Vec<String>>,
    /// Replacement discovered routes list (full replace).
    pub discovered_routes: Option<Vec<String>>,
    /// Regions to discover (append + dedup).
    pub discover_regions: Option<Vec<String>>,
    /// Routes to discover (append + dedup).
    pub discover_routes: Option<Vec<String>>,
    /// Per-character/NPC HP deltas.
    pub hp_changes: Option<HashMap<String, i32>>,
    /// NPC attitude string changes.
    pub npc_attitudes: Option<HashMap<String, String>>,
    /// NPC upsert patches.
    pub npcs_present: Option<Vec<NpcPatch>>,
    /// Active narrative stakes.
    pub active_stakes: Option<String>,
    /// Lore fragments to establish.
    pub lore_established: Option<Vec<String>>,
}

/// Patch for NPC upsert — used in npcs_present.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcPatch {
    /// NPC name (merge key).
    #[serde(deserialize_with = "deserialize_non_blank")]
    pub name: String,
    /// Updated description.
    pub description: Option<String>,
    /// Updated personality.
    pub personality: Option<String>,
    /// NPC role.
    pub role: Option<String>,
    /// Pronouns (identity-locked).
    pub pronouns: Option<String>,
    /// Appearance (identity-locked).
    pub appearance: Option<String>,
    /// Current location.
    pub location: Option<String>,
}

/// Custom deserializer that rejects blank strings.
fn deserialize_non_blank<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.trim().is_empty() {
        Err(serde::de::Error::custom("name cannot be blank"))
    } else {
        Ok(s)
    }
}

/// Patch for combat state.
/// Story 2-7: Extended with in_combat, hp_changes, turn_order, etc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPatch {
    /// Whether to advance the combat round.
    #[serde(default)]
    pub advance_round: bool,
    /// Whether combat is active.
    pub in_combat: Option<bool>,
    /// Per-combatant HP deltas.
    pub hp_changes: Option<HashMap<String, i32>>,
    /// Turn order.
    pub turn_order: Option<Vec<String>>,
    /// Current turn holder.
    pub current_turn: Option<String>,
    /// Available player actions.
    pub available_actions: Option<Vec<String>>,
    /// Drama weight for pacing.
    pub drama_weight: Option<f64>,
}

/// Patch for chase state.
/// Story 2-7: Extended with separation, phase, event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChasePatch {
    /// Start a new chase with (type, escape_threshold).
    pub start: Option<(ChaseType, f64)>,
    /// Record an escape roll.
    pub roll: Option<f64>,
    /// Distance between pursuer and quarry.
    pub separation: Option<i32>,
    /// Current chase phase.
    pub phase: Option<String>,
    /// Chase event description.
    pub event: Option<String>,
}

/// Generate reactive GameMessages from a state delta (ADR-027).
///
/// Always includes PARTY_STATUS. Conditionally includes CHAPTER_MARKER,
/// MAP_UPDATE, COMBAT_EVENT based on what changed.
pub fn broadcast_state_changes(delta: &StateDelta, state: &GameSnapshot) -> Vec<GameMessage> {
    let mut messages = Vec::new();

    // Always send PARTY_STATUS after a turn
    let members: Vec<PartyMember> = state
        .characters
        .iter()
        .map(|c| PartyMember {
            player_id: String::new(),
            name: c.name().to_string(),
            current_hp: Combatant::hp(c),
            max_hp: Combatant::max_hp(c),
            statuses: c.core.statuses.clone(),
            class: c.char_class.as_str().to_string(),
            level: Combatant::level(c),
            portrait_url: None,
        })
        .collect();
    messages.push(GameMessage::PartyStatus {
        payload: PartyStatusPayload { members },
        player_id: String::new(),
    });

    // CHAPTER_MARKER if location changed
    if delta.location_changed() {
        messages.push(GameMessage::ChapterMarker {
            payload: ChapterMarkerPayload {
                title: delta.new_location().map(|s| s.to_string()),
            },
            player_id: String::new(),
        });
    }

    // MAP_UPDATE if regions discovered
    if delta.regions_changed() {
        let explored: Vec<ExploredLocation> = state
            .discovered_regions
            .iter()
            .enumerate()
            .map(|(i, name)| ExploredLocation {
                name: name.clone(),
                x: i as i32,
                y: 0,
                location_type: "region".to_string(),
                connections: vec![],
            })
            .collect();
        messages.push(GameMessage::MapUpdate {
            payload: MapUpdatePayload {
                current_location: state.location.clone(),
                region: state.current_region.clone(),
                explored,
                fog_bounds: None,
            },
            player_id: String::new(),
        });
    }

    // COMBAT_EVENT if combat state changed
    if delta.combat_changed() {
        messages.push(GameMessage::CombatEvent {
            payload: CombatEventPayload {
                in_combat: state.combat.in_combat(),
                enemies: state
                    .npcs
                    .iter()
                    .filter(|n| {
                        n.disposition.attitude()
                            == crate::disposition::Attitude::Hostile
                    })
                    .map(|n| CombatEnemy {
                        name: n.name().to_string(),
                        hp: Combatant::hp(n),
                        max_hp: Combatant::max_hp(n),
                        ac: Some(Combatant::ac(n)),
                    })
                    .collect(),
                turn_order: state.combat.turn_order().to_vec(),
                current_turn: state
                    .combat
                    .current_turn()
                    .unwrap_or("")
                    .to_string(),
            },
            player_id: String::new(),
        });
    }

    messages
}
