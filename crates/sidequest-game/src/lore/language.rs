//! Language knowledge — bridge between conlang and lore systems (story 11-10).

use std::collections::HashMap;

use crate::conlang::{GeneratedName, Morpheme};

use super::store::{LoreCategory, LoreFragment, LoreSource, LoreStore};

/// Record that a character learned a morpheme during gameplay.
///
/// Creates a [`LoreFragment`] with category [`LoreCategory::Language`] and
/// source [`LoreSource::GameEvent`], storing the morpheme and its meaning
/// along with character and language metadata.
///
/// Returns the fragment id on success.
pub fn record_language_knowledge(
    store: &mut LoreStore,
    morpheme: &Morpheme,
    character_id: &str,
    turn: u64,
) -> Result<String, String> {
    let id = format!(
        "lang-{}-{}-{}",
        character_id, morpheme.language_id, morpheme.morpheme
    );
    let content = format!(
        "Morpheme '{}' means '{}' in language {}",
        morpheme.morpheme, morpheme.meaning, morpheme.language_id
    );
    let mut metadata = HashMap::new();
    metadata.insert("character_id".to_string(), character_id.to_string());
    metadata.insert("language_id".to_string(), morpheme.language_id.clone());
    metadata.insert("morpheme".to_string(), morpheme.morpheme.clone());
    metadata.insert("meaning".to_string(), morpheme.meaning.clone());
    let fragment = LoreFragment::new(
        id.clone(),
        LoreCategory::Language,
        content,
        LoreSource::GameEvent,
        Some(turn),
        metadata,
    );
    store.add(fragment)?;
    Ok(id)
}

/// Record that a character learned a generated name's meaning during gameplay.
///
/// Creates a [`LoreFragment`] with category [`LoreCategory::Language`] and
/// source [`LoreSource::GameEvent`], storing the name and its gloss
/// along with character and language metadata.
///
/// Returns the fragment id on success.
pub fn record_name_knowledge(
    store: &mut LoreStore,
    name: &GeneratedName,
    character_id: &str,
    turn: u64,
) -> Result<String, String> {
    let id = format!("name-{}-{}-{}", character_id, name.language_id, name.name);
    let content = format!(
        "Name '{}' means '{}' in language {}",
        name.name, name.gloss, name.language_id
    );
    let mut metadata = HashMap::new();
    metadata.insert("character_id".to_string(), character_id.to_string());
    metadata.insert("language_id".to_string(), name.language_id.clone());
    metadata.insert("name".to_string(), name.name.clone());
    metadata.insert("gloss".to_string(), name.gloss.clone());
    let fragment = LoreFragment::new(
        id.clone(),
        LoreCategory::Language,
        content,
        LoreSource::GameEvent,
        Some(turn),
        metadata,
    );
    store.add(fragment)?;
    Ok(id)
}

/// Query what a character knows about a specific language.
///
/// Returns all [`LoreFragment`]s with category [`LoreCategory::Language`]
/// whose metadata matches both `character_id` and `language_id`.
pub fn query_language_knowledge<'a>(
    store: &'a LoreStore,
    character_id: &str,
    language_id: &str,
) -> Vec<&'a LoreFragment> {
    store
        .query_by_category(&LoreCategory::Language)
        .into_iter()
        .filter(|f| {
            f.metadata().get("character_id").map(|s| s.as_str()) == Some(character_id)
                && f.metadata().get("language_id").map(|s| s.as_str()) == Some(language_id)
        })
        .collect()
}

/// Query ALL language knowledge for a character across all languages.
///
/// Returns all [`LoreFragment`]s with category [`LoreCategory::Language`]
/// for the given `character_id`, regardless of language.
pub fn query_all_language_knowledge<'a>(
    store: &'a LoreStore,
    character_id: &str,
) -> Vec<&'a LoreFragment> {
    store
        .query_by_category(&LoreCategory::Language)
        .into_iter()
        .filter(|f| f.metadata().get("character_id").map(|s| s.as_str()) == Some(character_id))
        .collect()
}

/// Format language knowledge fragments into a narrator prompt section.
///
/// Groups learned morphemes and names by language, producing a vocabulary
/// reference the narrator can use to weave constructed language terms into
/// narration. Returns an empty string if no language knowledge exists.
pub fn format_language_knowledge_for_prompt(fragments: &[&LoreFragment]) -> String {
    if fragments.is_empty() {
        return String::new();
    }

    // Group by language_id
    let mut by_language: HashMap<&str, Vec<&LoreFragment>> = HashMap::new();
    for frag in fragments {
        if let Some(lang) = frag.metadata().get("language_id") {
            by_language.entry(lang.as_str()).or_default().push(frag);
        }
    }

    let mut lines = vec!["\n\nCONSTRUCTED LANGUAGE VOCABULARY:".to_string()];
    lines.push(
        "The character has learned these words and names. Use them naturally in narration \
         — NPCs from the associated culture should speak with these terms."
            .to_string(),
    );

    for (language_id, frags) in &by_language {
        lines.push(format!("\n{}:", language_id));
        for frag in frags {
            let meta = frag.metadata();
            if let Some(morpheme) = meta.get("morpheme") {
                let meaning = meta.get("meaning").map(|s| s.as_str()).unwrap_or("?");
                lines.push(format!("- {} = \"{}\"", morpheme, meaning));
            } else if let Some(name) = meta.get("name") {
                let gloss = meta.get("gloss").map(|s| s.as_str()).unwrap_or("?");
                lines.push(format!("- {} (name) = \"{}\"", name, gloss));
            }
        }
    }

    lines.join("\n")
}
