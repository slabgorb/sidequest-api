//! Game state composition — GameSnapshot and typed patches.
//!
//! Port lesson #4: GameSnapshot composes domain structs, no god object.
//! Each domain struct owns its mutations via typed patch application.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::achievement::AchievementTracker;
use crate::axis::AxisValue;
use crate::character::Character;
use crate::combatant::Combatant;
use crate::consequence::GenieWish;
use crate::creature_core::CreatureCore;
use crate::delta::StateDelta;
use crate::disposition::Disposition;
use crate::encounter::StructuredEncounter;
use crate::inventory::Inventory;
use crate::merchant::{
    self, MerchantError, MerchantTransaction, MerchantTransactionRequest, TransactionType,
};
use crate::narrative::NarrativeEntry;
use crate::npc::Npc;
pub use crate::resource_pool::{
    ResourcePatch, ResourcePatchError, ResourcePatchOp, ResourcePatchResult, ResourcePool,
    ResourceThreshold,
};
use crate::scenario_state::ScenarioState;
use crate::trope::TropeState;
use crate::turn::TurnManager;
use crate::world_materialization::{CampaignMaturity, HistoryChapter};

use sidequest_protocol::{
    ChapterMarkerPayload, CharacterState, ExploredLocation, GameMessage, MapUpdatePayload,
};

/// Room IDs the player has visited in room-graph navigation mode.
///
/// Wraps `HashSet<String>` but serializes as a **sorted** `Vec<String>` for
/// deterministic JSON output (story 19-2). Deserializes from `Vec<String>`.
#[derive(Debug, Clone, Default, Eq)]
pub struct DiscoveredRooms(pub HashSet<String>);

impl Serialize for DiscoveredRooms {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sorted: Vec<&String> = self.0.iter().collect();
        sorted.sort();
        sorted.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiscoveredRooms {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let vec = Vec::<String>::deserialize(deserializer)?;
        Ok(DiscoveredRooms(vec.into_iter().collect()))
    }
}

impl std::ops::Deref for DiscoveredRooms {
    type Target = HashSet<String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for DiscoveredRooms {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::iter::FromIterator<String> for DiscoveredRooms {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        DiscoveredRooms(iter.into_iter().collect())
    }
}

impl PartialEq for DiscoveredRooms {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<HashSet<String>> for DiscoveredRooms {
    fn eq(&self, other: &HashSet<String>) -> bool {
        self.0 == *other
    }
}

impl PartialEq<DiscoveredRooms> for HashSet<String> {
    fn eq(&self, other: &DiscoveredRooms) -> bool {
        *self == other.0
    }
}

/// The complete game state at a point in time.
///
/// Composes all domain types (port lesson #4). Serializable for persistence
/// and WebSocket broadcast. Port lesson #11: captures ALL client-visible fields,
/// not just characters/location/quest_log like the Python version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "GameSnapshotRaw")]
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
    /// Active structured encounter (story 16-2).
    /// Generalizes ChaseState — supports standoffs, negotiations, ship combat, etc.
    /// Old saves with a `chase` field are migrated to this field during deserialization.
    #[serde(default)]
    pub encounter: Option<StructuredEncounter>,
    /// Currently active narrative tropes (full state for persistence).
    /// Backward-compatible: old saves with Vec<String> IDs deserialize as empty
    /// (tropes get re-seeded on first turn).
    #[serde(default, deserialize_with = "deserialize_trope_states")]
    pub active_tropes: Vec<TropeState>,
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
    #[serde(default)]
    pub last_saved_at: Option<DateTime<Utc>>,
    /// Active narrative stakes description (story 2-7).
    #[serde(default)]
    pub active_stakes: String,
    /// Established lore fragments (story 2-7).
    #[serde(default)]
    pub lore_established: Vec<String>,
    /// Turns since last meaningful player action (story 6-3).
    /// Drives the engagement multiplier for trope progression pacing.
    #[serde(default)]
    pub turns_since_meaningful: u32,
    /// Total story beats fired across all tropes (story 6-6).
    /// Contributes to effective turn count for maturity calculation.
    #[serde(default)]
    pub total_beats_fired: u32,
    /// Campaign maturity level (story 6-6).
    #[serde(default)]
    pub campaign_maturity: CampaignMaturity,
    /// Applied history chapters based on campaign maturity (story 6-6).
    #[serde(default)]
    pub world_history: Vec<HistoryChapter>,
    /// NPC identity registry — lightweight entries for narrator prompt consistency.
    /// Persisted so NPC identities survive across sessions.
    #[serde(default)]
    pub npc_registry: Vec<crate::npc::NpcRegistryEntry>,
    /// Genie wishes — power-grab actions granted with ironic consequences (F9).
    #[serde(default)]
    pub genie_wishes: Vec<GenieWish>,
    /// Current narrative axis values (story F2/F10 — /tone command).
    /// Persisted so tone settings survive across sessions.
    #[serde(default)]
    pub axis_values: Vec<AxisValue>,
    /// Achievement tracker (story F7).
    #[serde(default)]
    pub achievement_tracker: AchievementTracker,
    /// Active scenario state (Epic 7 — whodunit, belief state, clues, accusations).
    /// None when no scenario is active.
    #[serde(default)]
    pub scenario_state: Option<ScenarioState>,
    /// Room IDs the player has visited in room-graph mode (story 19-2).
    /// Empty in region mode. Serializes as sorted Vec for deterministic JSON.
    #[serde(default)]
    pub discovered_rooms: DiscoveredRooms,
    /// True when the player character has died (HP reached 0 in combat).
    /// Set by the death detection in state_mutations. Persists across saves
    /// so the narrator can handle the death narration on the next turn.
    #[serde(default)]
    pub player_dead: bool,
    /// Named resource pools with thresholds (story 16-10).
    /// Keys are resource names (e.g., "luck", "heat").
    #[serde(default)]
    pub resources: HashMap<String, ResourcePool>,
}

/// Backward-compatible deserializer for active_tropes.
/// Old saves stored Vec<String> (just IDs). New saves store Vec<TropeState>.
/// If deserialization fails (old format), return empty vec — tropes will re-seed.
fn deserialize_trope_states<'de, D>(deserializer: D) -> Result<Vec<TropeState>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Deserialize;
    let value = serde_json::Value::deserialize(deserializer)?;
    match serde_json::from_value::<Vec<TropeState>>(value) {
        Ok(states) => Ok(states),
        Err(_) => Ok(vec![]), // Old format — will be re-seeded
    }
}

/// Minimal shape of the legacy `chase` field on pre-16-2 save files.
///
/// Story 16-2 collapsed `ChaseState` into `StructuredEncounter`, and story
/// 28-9 then deleted `StructuredEncounter::from_chase_state()` because new
/// encounters are built from `ConfrontationDef` or `apply_beat()` at runtime.
/// Neither story addressed the load-old-save path: real user save files on
/// disk from before 16-2 still carry a `chase: { ... }` block, and without
/// this shim they would deserialize into a snapshot with `encounter = None`
/// (silent mid-chase state loss).
///
/// This type intentionally does NOT use `deny_unknown_fields` — we want to
/// quietly drop the legacy chase fields we no longer model (chase_phase,
/// chase_event, rounds, structured_phase, outcome, actors, etc.) while
/// preserving the values that map cleanly onto the new encounter shape.
#[derive(Deserialize)]
struct LegacyChaseState {
    #[serde(default)]
    separation_distance: i32,
    #[serde(default)]
    goal: i32,
    #[serde(default)]
    escape_threshold: f64,
    #[serde(default)]
    beat: u32,
    #[serde(default)]
    resolved: bool,
}

/// Raw deserialization helper for GameSnapshot backward compatibility (story 16-2).
///
/// Handles migration of old saves that have a `chase` field but no `encounter`
/// field. The `From<GameSnapshotRaw> for GameSnapshot` impl converts the old
/// ChaseState into a StructuredEncounter during deserialization.
#[derive(Deserialize)]
struct GameSnapshotRaw {
    #[serde(default)]
    genre_slug: String,
    #[serde(default)]
    world_slug: String,
    #[serde(default)]
    characters: Vec<Character>,
    #[serde(default)]
    npcs: Vec<Npc>,
    #[serde(default)]
    location: String,
    #[serde(default)]
    time_of_day: String,
    #[serde(default)]
    quest_log: HashMap<String, String>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    narrative_log: Vec<NarrativeEntry>,
    #[serde(default)]
    encounter: Option<StructuredEncounter>,
    /// Legacy chase block from pre-16-2 saves. Migrated into `encounter`
    /// in `From<GameSnapshotRaw>` when the new field is absent.
    #[serde(default)]
    chase: Option<LegacyChaseState>,
    #[serde(default, deserialize_with = "deserialize_trope_states")]
    active_tropes: Vec<TropeState>,
    #[serde(default)]
    atmosphere: String,
    #[serde(default)]
    current_region: String,
    #[serde(default)]
    discovered_regions: Vec<String>,
    #[serde(default)]
    discovered_routes: Vec<String>,
    #[serde(default)]
    turn_manager: TurnManager,
    #[serde(default)]
    last_saved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    active_stakes: String,
    #[serde(default)]
    lore_established: Vec<String>,
    #[serde(default)]
    turns_since_meaningful: u32,
    #[serde(default)]
    total_beats_fired: u32,
    #[serde(default)]
    campaign_maturity: CampaignMaturity,
    #[serde(default)]
    world_history: Vec<HistoryChapter>,
    #[serde(default)]
    npc_registry: Vec<crate::npc::NpcRegistryEntry>,
    #[serde(default)]
    genie_wishes: Vec<GenieWish>,
    #[serde(default)]
    axis_values: Vec<AxisValue>,
    #[serde(default)]
    achievement_tracker: AchievementTracker,
    #[serde(default)]
    scenario_state: Option<ScenarioState>,
    #[serde(default)]
    resource_state: HashMap<String, f64>,
    #[serde(default)]
    resource_declarations: Vec<sidequest_genre::ResourceDeclaration>,
    #[serde(default)]
    discovered_rooms: DiscoveredRooms,
    #[serde(default)]
    player_dead: bool,
    #[serde(default)]
    resources: HashMap<String, ResourcePool>,
}

impl From<GameSnapshotRaw> for GameSnapshot {
    fn from(raw: GameSnapshotRaw) -> Self {
        // Resource system migration (phase 4 of resource consolidation).
        //
        // Old saves store resources in `resource_state: HashMap<String, f64>` with
        // metadata in a parallel `resource_declarations` vec. New saves store them
        // as `resources: HashMap<String, ResourcePool>`.
        //
        // If `resources` is populated, use it directly (new save). Otherwise,
        // synthesize minimal ResourcePool entries from the legacy fields so the
        // player's saved values survive deserialization. The next
        // `init_resource_pools()` call on session load will upsert the
        // genre-pack metadata (label, min, max, decay, thresholds) without
        // clobbering `current` — that's what phase 1a's upsert semantics enable.
        let resources = if !raw.resources.is_empty() {
            raw.resources
        } else if !raw.resource_state.is_empty() {
            let mut pools: HashMap<String, ResourcePool> = HashMap::new();
            for (name, current) in &raw.resource_state {
                // Look up metadata from legacy declarations if present;
                // otherwise synthesize unbounded defaults that init_resource_pools
                // will overwrite on the next session load.
                let decl = raw.resource_declarations.iter().find(|d| d.name == *name);
                let pool = if let Some(d) = decl {
                    ResourcePool {
                        name: d.name.clone(),
                        label: d.label.clone(),
                        current: *current,
                        min: d.min,
                        max: d.max,
                        voluntary: d.voluntary,
                        decay_per_turn: d.decay_per_turn,
                        thresholds: d
                            .thresholds
                            .iter()
                            .map(|t| crate::resource_pool::ResourceThreshold {
                                at: t.at,
                                event_id: t.event_id.clone(),
                                narrator_hint: t.narrator_hint.clone(),
                            })
                            .collect(),
                    }
                } else {
                    ResourcePool {
                        name: name.clone(),
                        label: String::new(), // upsert fills this on session load
                        current: *current,
                        min: f64::MIN,
                        max: f64::MAX,
                        voluntary: false,
                        decay_per_turn: 0.0,
                        thresholds: Vec::new(),
                    }
                };
                pools.insert(name.clone(), pool);
            }
            pools
        } else {
            HashMap::new()
        };

        Self {
            genre_slug: raw.genre_slug,
            world_slug: raw.world_slug,
            characters: raw.characters,
            npcs: raw.npcs,
            location: raw.location,
            time_of_day: raw.time_of_day,
            quest_log: raw.quest_log,
            notes: raw.notes,
            narrative_log: raw.narrative_log,
            // Migrate legacy `chase` field into the new `encounter` field
            // when the save predates story 16-2. The chase migration was
            // documented in the GameSnapshotRaw doc comment but the actual
            // shim was missing — story 28-9 deleted
            // StructuredEncounter::from_chase_state() and never replaced it.
            // See encounter_story_16_2_tests::old_chase_state_json_deserializes_as_encounter.
            encounter: raw.encounter.or_else(|| {
                raw.chase.map(|c| {
                    let mut enc = StructuredEncounter::chase(c.escape_threshold, None, c.goal);
                    enc.metric.current = c.separation_distance;
                    enc.beat = c.beat;
                    enc.resolved = c.resolved;
                    enc
                })
            }),
            active_tropes: raw.active_tropes,
            atmosphere: raw.atmosphere,
            current_region: raw.current_region,
            discovered_regions: raw.discovered_regions,
            discovered_routes: raw.discovered_routes,
            turn_manager: raw.turn_manager,
            last_saved_at: raw.last_saved_at,
            active_stakes: raw.active_stakes,
            lore_established: raw.lore_established,
            turns_since_meaningful: raw.turns_since_meaningful,
            total_beats_fired: raw.total_beats_fired,
            campaign_maturity: raw.campaign_maturity,
            world_history: raw.world_history,
            npc_registry: raw.npc_registry,
            genie_wishes: raw.genie_wishes,
            axis_values: raw.axis_values,
            achievement_tracker: raw.achievement_tracker,
            scenario_state: raw.scenario_state,
            discovered_rooms: raw.discovered_rooms,
            player_dead: raw.player_dead,
            resources,
        }
    }
}

impl GameSnapshot {
    /// Find the lowest HP ratio among friendly (player-controlled) characters.
    /// Returns 1.0 if no friendly characters exist.
    ///
    /// Delegates to `Combatant::hp_fraction()` so the `combatant.bloodied`
    /// OTEL watcher event added by story 35-10 is reachable from production
    /// state-build code (CLAUDE.md "No half-wired features"). The trait
    /// method already short-circuits on `max_hp == 0`, so the previous
    /// inline guard is no longer needed.
    pub fn lowest_friendly_hp_ratio(&self) -> f64 {
        use crate::combatant::Combatant;
        self.characters
            .iter()
            .filter(|c| c.is_friendly)
            .map(|c| c.edge_fraction())
            .fold(1.0_f64, f64::min)
    }

    /// Find a mutable character or NPC by name and apply an edge delta.
    ///
    /// Story 39-2: HP is gone — LLM-provided `hp_changes` patch fields are
    /// routed through `EdgePool::apply_delta` as a straight composure
    /// delta. Story 39-4 wires dispatch's real edge_delta channel.
    fn apply_hp_change(&mut self, name: &str, delta: i32) {
        for c in &mut self.characters {
            if c.name() == name {
                c.core.apply_edge_delta(delta);
                return;
            }
        }
        for n in &mut self.npcs {
            if n.name() == name {
                n.core.apply_edge_delta(delta);
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
            let quest_span = tracing::info_span!("quest_update", quest_count = ql.len(),);
            let _quest_guard = quest_span.enter();
            self.quest_log = ql.clone();
            changed.push("quest_log");
        }
        if let Some(ref updates) = patch.quest_updates {
            let existing_keys: std::collections::HashSet<&String> = self.quest_log.keys().collect();
            let added = updates
                .keys()
                .filter(|k| !existing_keys.contains(k))
                .count();
            let quest_span = tracing::info_span!("quest_update", quests_added = added,);
            let _quest_guard = quest_span.enter();
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

        // Discovered facts — route each fact to its character's known_facts (story 9-3)
        if let Some(ref facts) = patch.discovered_facts {
            for df in facts {
                if let Some(character) = self
                    .characters
                    .iter_mut()
                    .find(|c| c.name() == df.character_name)
                {
                    tracing::info!(
                        character = %df.character_name,
                        fact = %df.fact.content,
                        source = ?df.fact.source,
                        turn = df.fact.learned_turn,
                        "rag.discovered_fact_applied"
                    );
                    character.known_facts.push(df.fact.clone());
                } else {
                    tracing::warn!(
                        character = %df.character_name,
                        fact = %df.fact.content,
                        "rag.discovered_fact_orphaned — character not found"
                    );
                }
            }
            changed.push("discovered_facts");
        }

        // HP changes
        if let Some(ref hp) = patch.hp_changes {
            for (name, delta) in hp {
                self.apply_hp_change(name, *delta);
            }
            changed.push("hp_changes");
        }

        // NPC disposition deltas — numeric values from the LLM, applied directly
        if let Some(ref attitudes) = patch.npc_attitudes {
            for (name, &delta) in attitudes {
                for npc in &mut self.npcs {
                    if npc.name() == name {
                        npc.disposition.apply_delta(delta);
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
                            xp: 0,
                            statuses: vec![],
                            inventory: Inventory::default(),
                            edge: crate::creature_core::placeholder_edge_pool(),
                            acquired_advancements: vec![],
                        },
                        voice_id: None,
                        disposition: Disposition::new(0),
                        location: npc_patch
                            .location
                            .as_ref()
                            .and_then(|l| NonBlankString::new(l).ok()),
                        pronouns: npc_patch.pronouns.clone(),
                        appearance: npc_patch.appearance.clone(),
                        age: npc_patch.age.clone(),
                        build: npc_patch.build.clone(),
                        height: npc_patch.height.clone(),
                        distinguishing_features: npc_patch
                            .distinguishing_features
                            .clone()
                            .unwrap_or_default(),
                        ocean: None,
                        belief_state: crate::belief_state::BeliefState::default(),
                        resolution_tier: crate::npc::ResolutionTier::default(),
                        non_transactional_interactions: 0,
                        jungian_id: None,
                        rpg_role_id: None,
                        npc_role_id: None,
                        resolved_archetype: None,
                    };
                    self.npcs.push(new_npc);
                }
            }
            changed.push("npcs_present");
        }

        span.record(
            "fields_changed",
            tracing::field::display(&changed.join(",")),
        );
    }

    /// Apply merchant transactions mechanically via execute_buy/execute_sell.
    ///
    /// Each request is resolved using the named NPC's disposition for pricing.
    /// Returns one Result per request. Failed transactions leave state unchanged
    /// (atomic per-transaction). Emits OTEL spans for each successful transaction.
    ///
    /// Uses characters[0] as the player (single-player assumption matches
    /// the existing inventory and gold tracking pattern).
    pub fn apply_merchant_transactions(
        &mut self,
        requests: &[MerchantTransactionRequest],
    ) -> Vec<Result<MerchantTransaction, MerchantError>> {
        /// Default carry limit for player inventory during merchant transactions.
        const PLAYER_CARRY_LIMIT: usize = 100;

        let mut results = Vec::with_capacity(requests.len());

        for request in requests {
            // Find the merchant NPC by name
            let merchant_idx = self
                .npcs
                .iter()
                .position(|n| n.name() == request.merchant_name);

            let Some(merchant_idx) = merchant_idx else {
                results.push(Err(MerchantError::CharacterNotFound(
                    request.merchant_name.clone(),
                )));
                continue;
            };

            // Copy disposition before mutable borrows (Disposition is Copy)
            let disposition = self.npcs[merchant_idx].disposition;
            let gold_before = self.characters[0].core.inventory.gold;

            // Split borrow: player is in characters, merchant is in npcs — no overlap
            let player_inv = &mut self.characters[0].core.inventory;
            let merchant_inv = &mut self.npcs[merchant_idx].core.inventory;

            let result = match request.transaction_type {
                TransactionType::Buy => merchant::execute_buy(
                    player_inv,
                    merchant_inv,
                    &request.item_id,
                    &disposition,
                    PLAYER_CARRY_LIMIT,
                ),
                TransactionType::Sell => {
                    merchant::execute_sell(player_inv, merchant_inv, &request.item_id, &disposition)
                }
            };

            // Emit OTEL span for successful transactions
            if let Ok(ref tx) = result {
                let gold_after = self.characters[0].core.inventory.gold;
                let tx_type = format!("{:?}", tx.transaction_type);
                let _span = tracing::info_span!(
                    "merchant.transaction",
                    transaction_type = %tx_type,
                    item_name = %tx.item_name,
                    price = tx.price,
                    gold_before = gold_before,
                    gold_after = gold_after,
                )
                .entered();
            }

            results.push(result);
        }

        results
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
    /// NPC disposition deltas (signed integers on the -100 to +100 scale).
    pub npc_attitudes: Option<HashMap<String, i32>>,
    /// NPC upsert patches.
    pub npcs_present: Option<Vec<NpcPatch>>,
    /// Active narrative stakes.
    pub active_stakes: Option<String>,
    /// Lore fragments to establish.
    pub lore_established: Option<Vec<String>>,
    /// Facts discovered this turn (Story 9-3).
    pub discovered_facts: Option<Vec<crate::known_fact::DiscoveredFact>>,
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
    /// Age description (identity-locked).
    pub age: Option<String>,
    /// Body build descriptor (identity-locked).
    pub build: Option<String>,
    /// Relative height descriptor (identity-locked).
    pub height: Option<String>,
    /// Specific visual details (identity-locked).
    pub distinguishing_features: Option<Vec<String>>,
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

// CombatPatch and ChasePatch deleted in story 28-9.
// StructuredEncounter is the sole encounter mutation model.

/// Generate reactive GameMessages from a state delta (ADR-027).
///
/// Always includes PARTY_STATUS. Conditionally includes CHAPTER_MARKER,
/// MAP_UPDATE, COMBAT_EVENT based on what changed.
pub fn broadcast_state_changes(delta: &StateDelta, state: &GameSnapshot) -> Vec<GameMessage> {
    let mut messages = Vec::new();

    // PARTY_STATUS is built with full player context in
    // `sidequest-server/src/dispatch/response.rs::build_response_messages`,
    // which has access to `ctx.player_id`, the lobby name, and the cached
    // PlayerState sheet/inventory. This function historically ALSO
    // constructed a PartyStatus with blank placeholders for player_id/name,
    // producing two competing PartyStatus messages per turn — one with real
    // values, one with sentinels — which the React reducer coalesced based
    // on arrival order. The NonBlankString protocol sweep made the
    // placeholders impossible to construct, which surfaced the duplication.
    //
    // Dropping the PartyStatus emission here eliminates the competing
    // broadcast; the server-side build remains the single source of truth.
    // This function still owns the bloodied OTEL emission below and the
    // ChapterMarker / MapUpdate broadcasts, which have no server-side twin.

    // OTEL: combatant.bloodied — emitted when this turn mutated character
    // state and any friendly is now below half HP. Transition-site emission
    // follows the disposition::apply_delta precedent: telemetry at the
    // mutation/ship point, never inside a pure accessor. broadcast_state_changes
    // is dispatched from sidequest-server/src/dispatch/mod.rs:1737 every turn,
    // so this is the canonical place for the GM panel to observe combat
    // engagement (CLAUDE.md OTEL Observability Principle, story 35-10).
    if delta.characters_changed() {
        for c in state.characters.iter().filter(|c| c.is_friendly) {
            let edge = Combatant::edge(c);
            let max_edge = Combatant::max_edge(c);
            if max_edge == 0 {
                continue;
            }
            let frac = edge as f64 / max_edge as f64;
            if frac < 0.5 {
                WatcherEventBuilder::new("combatant", WatcherEventType::StateTransition)
                    .field("action", "strained")
                    .field("name", c.name())
                    .field("edge", edge)
                    .field("max_edge", max_edge)
                    .field("edge_fraction", frac)
                    .send();
            }
        }
    }

    // CHAPTER_MARKER if location changed
    if delta.location_changed() {
        messages.push(GameMessage::ChapterMarker {
            payload: ChapterMarkerPayload {
                title: delta.new_location().map(|s| s.to_string()),
                location: delta.new_location().map(|s| s.to_string()),
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
            .filter_map(|(i, region_name)| {
                // Drop any blank region names rather than silently emitting
                // empty map markers — a blank region is a data bug upstream
                // (cartography registration), not a valid map entry.
                let name = NonBlankString::new(region_name).ok()?;
                Some(ExploredLocation {
                    // Region mode has no separate slug — id mirrors name.
                    id: region_name.clone(),
                    name,
                    x: i as i32,
                    y: 0,
                    location_type: "region".to_string(),
                    connections: vec![],
                    room_exits: vec![],
                    room_type: String::new(),
                    size: None,
                    is_current_room: false,
                    tactical_grid: None,
                })
            })
            .collect();
        // Likewise require non-blank current_location / region for the
        // outer MapUpdate — if the game state doesn't have a real location
        // yet, skip the broadcast entirely. The client's last-known map
        // state stays on screen.
        if let (Ok(current_location), Ok(region)) = (
            NonBlankString::new(&state.location),
            NonBlankString::new(&state.current_region),
        ) {
            messages.push(GameMessage::MapUpdate {
                payload: MapUpdatePayload {
                    current_location,
                    region,
                    explored,
                    fog_bounds: None,
                    cartography: None,
                },
                player_id: String::new(),
            });
        } else {
            tracing::warn!(
                location = %state.location,
                region = %state.current_region,
                "map_update_skipped — state.location or state.current_region is blank; \
                 cannot construct a non-blank MapUpdatePayload"
            );
        }
    }

    // COMBAT_EVENT removed in story 28-9 — ConfrontationPayload replaces it.

    messages
}

/// Build the wire-format StateDelta from a game-crate delta and current snapshot.
///
/// Converts the boolean-flagged game delta into the protocol's data-carrying delta
/// that the client uses to update its state mirror. Story 15-20: replaces inline
/// construction in dispatch/mod.rs.
pub fn build_protocol_delta(
    delta: &StateDelta,
    state: &GameSnapshot,
    items_gained: &[sidequest_protocol::ItemGained],
) -> sidequest_protocol::StateDelta {
    let span = tracing::info_span!(
        "build_protocol_delta",
        location_changed = delta.location_changed(),
        characters_changed = delta.characters_changed(),
        quest_log_changed = delta.quest_log_changed(),
        items_gained_count = items_gained.len(),
    );
    let _guard = span.enter();

    sidequest_protocol::StateDelta {
        location: if delta.location_changed() {
            Some(state.location.clone())
        } else {
            None
        },
        characters: if delta.characters_changed() {
            Some(
                state
                    .characters
                    .iter()
                    .map(|c| CharacterState {
                        // `CreatureCore.name` is already `NonBlankString` —
                        // clone the validated newtype directly instead of
                        // round-tripping through `&str` → `String`.
                        name: c.core.name.clone(),
                        // Protocol still carries `hp`/`max_hp` field names in
                        // 39-2; story 39-7 renames on the wire + UI side.
                        // Until then we surface edge values through them.
                        hp: Combatant::edge(c),
                        max_hp: Combatant::max_edge(c),
                        level: Combatant::level(c),
                        // CharacterState.class is kept as raw String on the
                        // protocol side (see message.rs comment) — unwrap
                        // the source NonBlankString for this one field.
                        class: c.char_class.as_str().to_string(),
                        statuses: vec![],
                        inventory: c
                            .core
                            .inventory
                            .items
                            .iter()
                            .map(|i| i.name.as_str().to_string())
                            .collect(),
                        archetype_provenance: c.archetype_provenance.clone(),
                    })
                    .collect(),
            )
        } else {
            None
        },
        quests: if delta.quest_log_changed() {
            Some(state.quest_log.clone())
        } else {
            None
        },
        items_gained: if items_gained.is_empty() {
            None
        } else {
            Some(items_gained.to_vec())
        },
    }
}
