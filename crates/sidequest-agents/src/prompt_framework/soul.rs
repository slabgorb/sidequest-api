//! SOUL.md parser — extracts guiding principles for agent prompt injection.
//!
//! Parses bold-header paragraphs (`**Name.** Body text`) from SOUL.md into
//! [`SoulPrinciple`] objects. Ports Python `sidequest/soul.py`.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A named guiding principle from SOUL.md.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulPrinciple {
    /// Principle name (e.g., "Agency", "Living World").
    pub name: String,
    /// Principle body text.
    pub text: String,
}

/// Parsed SOUL.md structure — all principles in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulData {
    /// The principles extracted from SOUL.md, in document order.
    pub principles: Vec<SoulPrinciple>,
    /// The document title (first `# ` heading), if present.
    pub title: Option<String>,
    /// The subtitle/description (text between title and first principle).
    pub description: Option<String>,
}

impl SoulData {
    /// Returns the number of principles.
    pub fn len(&self) -> usize {
        todo!("SoulData::len")
    }

    /// Returns true if there are no principles.
    pub fn is_empty(&self) -> bool {
        todo!("SoulData::is_empty")
    }

    /// Look up a principle by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&SoulPrinciple> {
        todo!("SoulData::get")
    }

    /// Format all principles as a bullet list for prompt injection.
    pub fn as_prompt_text(&self) -> String {
        todo!("SoulData::as_prompt_text")
    }
}

/// Parse a SOUL.md file and return the structured data.
///
/// Returns an empty `SoulData` if the file does not exist.
/// Extracts `**Name.** Body text` patterns (same regex as Python).
pub fn parse_soul_md(path: &Path) -> SoulData {
    todo!("parse_soul_md")
}
