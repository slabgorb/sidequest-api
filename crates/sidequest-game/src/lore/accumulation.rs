//! Lore accumulation — create fragments from game events (story 11-5).

use std::collections::HashMap;

use super::store::{LoreCategory, LoreFragment, LoreSource, LoreStore};

/// Create a lore fragment from a game event and add it to the store.
///
/// Returns the fragment id on success, or an error if the description is empty
/// or the fragment could not be added.
pub fn accumulate_lore(
    store: &mut LoreStore,
    event_description: &str,
    category: LoreCategory,
    turn: u64,
    metadata: HashMap<String, String>,
) -> Result<String, String> {
    if event_description.is_empty() {
        return Err("event description must not be empty".to_string());
    }

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    event_description.hash(&mut hasher);
    turn.hash(&mut hasher);
    let hash = hasher.finish();
    let id = format!("evt-{turn}-{hash:016x}");

    let fragment = LoreFragment::new(
        id.clone(),
        category,
        event_description.to_string(),
        LoreSource::GameEvent,
        Some(turn),
        metadata,
    );
    store.add(fragment)?;
    Ok(id)
}

/// Batch version of [`accumulate_lore`] — processes multiple events at once.
pub fn accumulate_lore_batch(
    store: &mut LoreStore,
    events: &[(String, LoreCategory, u64, HashMap<String, String>)],
) -> Vec<Result<String, String>> {
    events
        .iter()
        .map(|(desc, cat, turn, meta)| {
            accumulate_lore(store, desc, cat.clone(), *turn, meta.clone())
        })
        .collect()
}
