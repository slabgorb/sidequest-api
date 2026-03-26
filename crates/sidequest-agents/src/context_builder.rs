//! Composable context builder for agent prompt assembly.
//!
//! Port lesson #8: ContextBuilder with composable sections replaces
//! manual format-helper assembly in each agent.

use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// Builder for assembling agent context from composable sections.
///
/// Sections are added in any order; `build()` returns them sorted by
/// attention zone (primacy → recency).
#[derive(Debug, Default)]
pub struct ContextBuilder {
    sections: Vec<PromptSection>,
}

impl ContextBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a section to the builder.
    pub fn add_section(&mut self, section: PromptSection) {
        self.sections.push(section);
    }

    /// Build and return sections sorted by attention zone order.
    pub fn build(&self) -> Vec<PromptSection> {
        let mut sorted = self.sections.clone();
        sorted.sort_by_key(|s| s.zone);
        sorted
    }

    /// Compose all sections into a single string, ordered by zone.
    /// Emits a tracing span with sections_count, total_tokens, and zone_distribution (story 3-1).
    pub fn compose(&self) -> String {
        let sections_count = self.sections.len();
        let total_tokens = self.token_estimate();

        // Build zone distribution string
        let zone_dist = {
            let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            for s in &self.sections {
                let zone_name = match s.zone {
                    AttentionZone::Primacy => "primacy",
                    AttentionZone::Early => "early",
                    AttentionZone::Valley => "valley",
                    AttentionZone::Late => "late",
                    AttentionZone::Recency => "recency",
                };
                *counts.entry(zone_name).or_insert(0) += 1;
            }
            let mut parts: Vec<String> = counts
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            parts.sort();
            parts.join(",")
        };

        let span = tracing::info_span!(
            "compose",
            sections_count = sections_count,
            total_tokens = total_tokens,
            zone_distribution = %zone_dist,
        );
        let _guard = span.enter();

        self.build()
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Filter sections by category.
    pub fn sections_by_category(&self, category: SectionCategory) -> Vec<&PromptSection> {
        self.sections
            .iter()
            .filter(|s| s.category == category)
            .collect()
    }

    /// Filter sections by attention zone.
    pub fn sections_by_zone(&self, zone: AttentionZone) -> Vec<&PromptSection> {
        self.sections.iter().filter(|s| s.zone == zone).collect()
    }

    /// Estimate total token count across all sections.
    pub fn token_estimate(&self) -> usize {
        self.sections.iter().map(|s| s.token_estimate()).sum()
    }
}
