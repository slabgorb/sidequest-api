//! Confirmation-phase summary rendering for the character builder.
//!
//! Until 2026-04-09 this lived inside `CharacterBuilder::to_scene_message` in
//! the `sidequest-game` crate. That was the wrong home: the builder is a
//! state machine, and a faithful confirmation summary needs inputs the
//! builder does not own — specifically the **lobby-provided player name**
//! (there is no chargen scene for it in genres like `caverns_and_claudes`)
//! and the **genre pack's `starting_equipment` table** (resolved from
//! `inventory.yaml`, not from scene effects).
//!
//! Keeping summary rendering inside the builder silently dropped those two
//! fields during the Thessa playtest bug on 2026-04-09. Moving it here
//! co-locates rendering with the data it requires: the server-side dispatch
//! layer already holds the `GenrePack` and the lobby name at every chargen
//! call site.
//!
//! The builder stays responsible for the state machine; this module is the
//! view. New summary fields go here, not in the builder.

use sidequest_game::builder::{humanize_snake_case, CharacterBuilder};
use sidequest_genre::GenrePack;
use sidequest_protocol::{CharacterCreationPayload, GameMessage};

use crate::{WatcherEventBuilder, WatcherEventType};

/// Which source produced the Name line in the rendered summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameSource {
    /// A freeform name-entry scene in the builder (e.g. mutant_wasteland).
    NameScene,
    /// The lobby-provided name passed via the `connect` payload.
    Lobby,
    /// No name available from either source.
    None,
}

impl NameSource {
    fn as_str(self) -> &'static str {
        match self {
            NameSource::NameScene => "name_scene",
            NameSource::Lobby => "lobby",
            NameSource::None => "none",
        }
    }
}

/// Which source produced the Equipment line in the rendered summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EquipmentSource {
    /// Accumulated `item_hint` mechanical effects from scene choices.
    SceneItemHints,
    /// Looked up from `pack.inventory.starting_equipment[class]`.
    PackStartingEquipment,
    /// Both sources contributed (scene hints merged onto the class loadout).
    Merged,
    /// Neither source produced any equipment.
    None,
}

impl EquipmentSource {
    fn as_str(self) -> &'static str {
        match self {
            EquipmentSource::SceneItemHints => "scene_item_hints",
            EquipmentSource::PackStartingEquipment => "pack_starting_equipment",
            EquipmentSource::Merged => "merged",
            EquipmentSource::None => "none",
        }
    }
}

/// Render the Confirmation-phase summary message for a builder.
///
/// Pulls fields from three sources:
///
/// 1. **Builder state** — pronouns, stats, race/class hints, mutation/rig
///    traits, backstory, and the name-entry-scene name (if the genre has one).
/// 2. **Lobby name** — fallback for the Name line when no name-entry scene
///    exists. The precedence (scene > lobby) matches the precedence used at
///    `build()` time in `dispatch::connect::dispatch_character_creation`.
/// 3. **Genre pack inventory** — `starting_equipment[class]` resolved via
///    either the accumulated `class_hint` or, if absent, the genre's
///    `default_class` from `rules.yaml`. Item IDs are mapped to display
///    names through `pack.inventory.item_catalog` when possible.
///
/// Emits an OTEL span `character_creation.confirmation_rendered` recording
/// which sources fired, so the GM panel can catch silent regressions
/// (empty Name line, missing Equipment line, etc.).
pub fn render_confirmation_summary(
    builder: &CharacterBuilder,
    pack: &GenrePack,
    lobby_name: Option<&str>,
    player_id: &str,
) -> GameMessage {
    debug_assert!(
        builder.is_confirmation(),
        "render_confirmation_summary called outside Confirmation phase"
    );

    let acc = builder.accumulated();
    let mut parts: Vec<String> = Vec::new();

    // --- Name (scene > lobby > omit) --------------------------------------
    let (name_source, resolved_name) = match builder.character_name() {
        Some(n) => (NameSource::NameScene, Some(n.to_string())),
        None => match lobby_name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(n) => (NameSource::Lobby, Some(n.to_string())),
            None => (NameSource::None, None),
        },
    };
    if let Some(ref n) = resolved_name {
        parts.push(format!("Name: {}", n));
    }

    // --- Race / Class / Personality ---------------------------------------
    // Only show fields the chargen actually accumulated. Genres like
    // caverns_and_claudes deliberately omit race/class scenes — we don't lie
    // with "Unknown" for fields the genre doesn't define.
    if let Some(ref r) = acc.race_hint {
        parts.push(format!("{}: {}", builder.race_label(), r));
    }
    if let Some(ref c) = acc.class_hint {
        parts.push(format!("{}: {}", builder.class_label(), c));
    } else if let Some(dc) = builder.default_class() {
        // If the genre has a default_class in rules.yaml (e.g. caverns
        // default_class: Delver), show it on the summary so the player sees
        // what class their equipment will be loaded for.
        parts.push(format!("{}: {}", builder.class_label(), dc));
    }
    if let Some(ref p) = acc.personality_trait {
        parts.push(format!("Personality: {}", p));
    }
    if let Some(ref pn) = acc.pronoun_hint {
        parts.push(format!("Pronouns: {}", pn));
    }

    // --- Stats ------------------------------------------------------------
    if let Some(rolled) = builder.rolled_stats() {
        let stat_line = rolled
            .iter()
            .map(|(name, val)| format!("{} {}", name, val))
            .collect::<Vec<_>>()
            .join("  ");
        parts.push(format!("Stats: {}", stat_line));
    }

    if let Some(ref m) = acc.mutation_hint {
        parts.push(format!("Mutation: {}", humanize_snake_case(m)));
    }
    if let Some(ref a) = acc.affinity_hint {
        parts.push(format!("Affinity: {}", a));
    }
    if let Some(ref r) = acc.rig_type_hint {
        parts.push(format!("Rig: {}", r));
    }
    if let Some(ref rt) = acc.rig_trait {
        parts.push(format!("Rig Trait: {}", rt));
    }

    // --- Equipment (merge scene hints with pack starting equipment) -------
    // Resolve the class used for the starting_equipment lookup the same way
    // `dispatch_character_creation`'s confirmation branch does at build time:
    // prefer an explicit class_hint, otherwise fall back to the genre's
    // default_class from rules.yaml. This keeps the *preview* and the
    // *actual wired character* in sync by construction — no drift.
    let lookup_class: Option<String> = acc
        .class_hint
        .clone()
        .or_else(|| builder.default_class().map(str::to_string));

    let mut equipment_ids: Vec<String> = Vec::new();
    let mut used_scene_hints = false;
    let mut used_pack_starting = false;

    if let Some(ref inv) = pack.inventory {
        if let Some(ref class_name) = lookup_class {
            let class_lower = class_name.to_lowercase();
            if let Some((_, loadout)) = inv
                .starting_equipment
                .iter()
                .find(|(k, _)| k.to_lowercase() == class_lower)
            {
                equipment_ids.extend(loadout.iter().cloned());
                used_pack_starting = !loadout.is_empty();
            }
        }
    }
    if !acc.item_hints.is_empty() {
        for hint in &acc.item_hints {
            if !equipment_ids.iter().any(|e| e == hint) {
                equipment_ids.push(hint.clone());
            }
        }
        used_scene_hints = true;
    }

    let equipment_source = match (used_pack_starting, used_scene_hints) {
        (true, true) => EquipmentSource::Merged,
        (true, false) => EquipmentSource::PackStartingEquipment,
        (false, true) => EquipmentSource::SceneItemHints,
        (false, false) => EquipmentSource::None,
    };

    if !equipment_ids.is_empty() {
        let display_items: Vec<String> = equipment_ids
            .iter()
            .map(|id| resolve_item_display_name(pack, id))
            .collect();
        parts.push(format!("Equipment: {}", display_items.join(", ")));
    }

    if let Some(bg) = &acc.background {
        parts.push(format!("\nBackstory: {}", bg));
    }

    let summary = parts.join("\n");

    // --- Lie-detector telemetry -------------------------------------------
    // Records which sources fired so the GM panel can catch silent drops
    // (e.g. the 2026-04-09 Thessa bug: name_source=none, equipment_source=none
    // despite a lobby name being present and the pack defining a Delver
    // loadout).
    WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
        .field("event", "confirmation_rendered")
        .field("name_source", name_source.as_str())
        .field("has_name", resolved_name.is_some())
        .field("equipment_source", equipment_source.as_str())
        .field("equipment_count", equipment_ids.len() as i64)
        .field("lookup_class", lookup_class.as_deref().unwrap_or(""))
        .field("has_rolled_stats", builder.rolled_stats().is_some())
        .field("player_id", player_id)
        .send();

    GameMessage::CharacterCreation {
        payload: CharacterCreationPayload {
            phase: "confirmation".to_string(),
            scene_index: None,
            total_scenes: Some(builder.total_scenes() as u32),
            prompt: None,
            summary: Some(summary),
            message: None,
            choices: None,
            allows_freeform: None,
            input_type: None,
            loading_text: None,
            character_preview: None,
            rolled_stats: None,
            choice: None,
            character: None,
        },
        player_id: player_id.to_string(),
    }
}

/// Map a starting-equipment item ID to a display name via
/// `pack.inventory.item_catalog`, falling back to Title-Cased snake_case if
/// the catalog has no entry.
fn resolve_item_display_name(pack: &GenrePack, item_id: &str) -> String {
    if let Some(ref inv) = pack.inventory {
        if let Some(entry) = inv.item_catalog.iter().find(|c| c.id == item_id) {
            if !entry.name.is_empty() {
                return entry.name.clone();
            }
        }
    }
    humanize_snake_case(item_id)
}
