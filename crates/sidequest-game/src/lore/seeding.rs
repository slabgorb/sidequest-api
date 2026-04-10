//! Bootstrap a [`LoreStore`] from genre pack + character creation data (story 11-3).

use std::collections::HashMap;

use sidequest_genre::{CharCreationScene, GenrePack};

use super::store::{LoreCategory, LoreFragment, LoreSource, LoreStore};

/// Seed a [`LoreStore`] with lore fragments derived from a genre pack's lore
/// section (history, geography, cosmology, factions).
///
/// Returns the number of fragments added.
pub fn seed_lore_from_genre_pack(store: &mut LoreStore, genre_pack: &GenrePack) -> usize {
    let mut count = 0;

    if !genre_pack.lore.history.is_empty() {
        let frag = LoreFragment::new(
            "lore_genre_history".to_string(),
            LoreCategory::History,
            genre_pack.lore.history.clone(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        if store.add(frag).is_ok() {
            count += 1;
        }
    }

    if !genre_pack.lore.geography.is_empty() {
        let frag = LoreFragment::new(
            "lore_genre_geography".to_string(),
            LoreCategory::Geography,
            genre_pack.lore.geography.clone(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        if store.add(frag).is_ok() {
            count += 1;
        }
    }

    if !genre_pack.lore.cosmology.is_empty() {
        let frag = LoreFragment::new(
            "lore_genre_cosmology".to_string(),
            LoreCategory::History,
            genre_pack.lore.cosmology.clone(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        if store.add(frag).is_ok() {
            count += 1;
        }
    }

    for faction in &genre_pack.lore.factions {
        let slug = faction.name.to_lowercase().replace(' ', "_");
        let mut metadata = HashMap::new();
        metadata.insert("faction_name".to_string(), faction.name.clone());
        let frag = LoreFragment::new(
            format!("lore_genre_faction_{slug}"),
            LoreCategory::Faction,
            format!("{}: {}", faction.name, faction.description),
            LoreSource::GenrePack,
            None,
            metadata,
        );
        if store.add(frag).is_ok() {
            count += 1;
        }
    }

    count
}

/// Seed a [`LoreStore`] with lore fragments derived from character creation
/// scene choices.
///
/// Returns the number of fragments added.
pub fn seed_lore_from_char_creation(
    store: &mut LoreStore,
    scenes: &[CharCreationScene],
) -> usize {
    let mut count = 0;

    for scene in scenes {
        for (index, choice) in scene.choices.iter().enumerate() {
            let mut metadata = HashMap::new();
            metadata.insert("scene_id".to_string(), scene.id.clone());
            metadata.insert("choice_index".to_string(), index.to_string());
            metadata.insert("choice_label".to_string(), choice.label.clone());

            let frag = LoreFragment::new(
                format!("lore_char_creation_{}_{}", scene.id, index),
                LoreCategory::Character,
                format!("{}: {}", choice.label, choice.description),
                LoreSource::CharacterCreation,
                None,
                metadata,
            );
            if store.add(frag).is_ok() {
                count += 1;
            }
        }
    }

    count
}
