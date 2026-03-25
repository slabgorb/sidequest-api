//! SOUL.md parser — extracts guiding principles for agent prompt injection.
//!
//! Parses bold-header paragraphs (`**Name.** Body text`) from SOUL.md into
//! [`SoulPrinciple`] objects. Ports Python `sidequest/soul.py`.

use regex::Regex;
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
        self.principles.len()
    }

    /// Returns true if there are no principles.
    pub fn is_empty(&self) -> bool {
        self.principles.is_empty()
    }

    /// Look up a principle by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&SoulPrinciple> {
        let lower = name.to_lowercase();
        self.principles
            .iter()
            .find(|p| p.name.to_lowercase() == lower)
    }

    /// Format all principles as a bullet list for prompt injection.
    pub fn as_prompt_text(&self) -> String {
        self.principles
            .iter()
            .map(|p| format!("- {}: {}", p.name, p.text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Parse a SOUL.md file and return the structured data.
///
/// Returns an empty `SoulData` if the file does not exist.
/// Extracts `**Name.** Body text` patterns (same regex as Python).
pub fn parse_soul_md(path: &Path) -> SoulData {
    let empty = SoulData {
        principles: Vec::new(),
        title: None,
        description: None,
    };

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return empty,
    };

    if content.is_empty() {
        return empty;
    }

    // Extract title from first `# ` heading.
    let title = content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string());

    // Extract description: text between title line and first principle.
    let description = extract_description(&content);

    // Extract **Name.** Body text patterns.
    let re = Regex::new(r"\*\*([^*]+?)\.\*\*\s*(.+)").unwrap();
    let principles: Vec<SoulPrinciple> = re
        .captures_iter(&content)
        .map(|cap| SoulPrinciple {
            name: cap[1].to_string(),
            text: cap[2].trim().to_string(),
        })
        .collect();

    SoulData {
        principles,
        title,
        description,
    }
}

/// Extract description text between the title line and the first `**bold**` principle.
fn extract_description(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    // Find title line index.
    let title_idx = lines.iter().position(|l| l.starts_with("# "))?;

    // Find first principle line.
    let first_principle_idx = lines
        .iter()
        .position(|l| l.starts_with("**") && l.contains(".**"));

    let end = first_principle_idx.unwrap_or(lines.len());

    // Collect non-empty lines between title and first principle.
    let desc_lines: Vec<&str> = lines[(title_idx + 1)..end]
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();

    if desc_lines.is_empty() {
        None
    } else {
        Some(desc_lines.join(" "))
    }
}
