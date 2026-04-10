//! Lore subsystem — indexed narrative knowledge for RAG-style context injection.
//!
//! This module is split by concern:
//! - [`store`] — core types: `LoreStore`, `LoreFragment`, `LoreCategory`, `LoreSource`
//! - [`similarity`] — cosine similarity for semantic search
//! - [`seeding`] — bootstrap the store from genre pack + character creation data
//! - [`retrieval`] — budget-aware fragment selection, telemetry, prompt formatting
//! - [`accumulation`] — create fragments from game events
//! - [`language`] — bridge between conlang and lore for learned-morpheme tracking
//!
//! Story 11-1 through 11-10, 18-4.

pub mod accumulation;
pub mod language;
pub mod retrieval;
pub mod seeding;
pub mod similarity;
pub mod store;

pub use accumulation::{accumulate_lore, accumulate_lore_batch};
pub use language::{
    format_language_knowledge_for_prompt, query_all_language_knowledge, query_language_knowledge,
    record_language_knowledge, record_name_knowledge,
};
pub use retrieval::{
    format_lore_context, select_lore_for_prompt, summarize_lore_retrieval, FragmentSummary,
    LoreRetrievalSummary,
};
pub use seeding::{seed_lore_from_char_creation, seed_lore_from_genre_pack};
pub use similarity::cosine_similarity;
pub use store::{LoreCategory, LoreFragment, LoreSource, LoreStore};

#[cfg(test)]
mod tests;
