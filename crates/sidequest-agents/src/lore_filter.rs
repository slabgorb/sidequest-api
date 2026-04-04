//! Story 23-4: LoreFilter — graph-distance + intent-based context retrieval.
//!
//! Determines which lore sections to inject into the narrator prompt's Valley
//! zone per turn. Primary signal: graph distance from current node. Secondary
//! signals: intent classification, NPC presence, arc proximity.

use std::collections::{HashMap, VecDeque};

use sidequest_game::npc::NpcRegistryEntry;
use sidequest_genre::WorldGraph;

use crate::agents::intent_router::Intent;

/// Detail level for a lore entity in the narrator prompt.
///
/// Ordered: Full > Summary > NameOnly, so `max()` picks the richest detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DetailLevel {
    /// Name only — closed-world assertion, prevents hallucination.
    NameOnly = 0,
    /// One-line summary (~10 tokens). Safe fallback from tiered summaries (23-2).
    Summary = 1,
    /// Full description, NPCs, items, state.
    Full = 2,
}

/// A single lore entity selected for prompt injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoreSelection {
    /// Slug identifier for the entity (e.g., "crown_remnant", "solenne").
    pub entity_id: String,
    /// Display name (e.g., "Crown Remnant", "Solenne").
    pub entity_name: String,
    /// Category: "faction", "culture", "location", "backstory".
    pub category: String,
    /// How much detail to inject.
    pub detail_level: DetailLevel,
    /// Why this detail level was chosen (for OTEL tracing).
    pub reason: String,
}

/// Graph-distance + intent-based lore filter for the narrator prompt.
///
/// Consumes the world graph (from cartography config) and determines per-turn
/// which lore entities to inject at which detail level.
pub struct LoreFilter<'a> {
    graph: &'a WorldGraph,
}

impl<'a> LoreFilter<'a> {
    /// Create a new filter backed by the given world graph.
    pub fn new(graph: &'a WorldGraph) -> Self {
        Self { graph }
    }

    /// BFS shortest-path distance between two nodes (bidirectional edges).
    ///
    /// Returns `None` if either node doesn't exist in the graph or is unreachable.
    pub fn graph_distance(&self, from: &str, to: &str) -> Option<usize> {
        // Both nodes must exist in the graph.
        if !self.graph.nodes.iter().any(|n| n.id == from) {
            return None;
        }
        if !self.graph.nodes.iter().any(|n| n.id == to) {
            return None;
        }

        if from == to {
            return Some(0);
        }

        // Standard BFS.
        let mut visited: HashMap<&str, usize> = HashMap::new();
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();

        visited.insert(from, 0);
        queue.push_back((from, 0));

        while let Some((current, dist)) = queue.pop_front() {
            // Use WorldGraph::neighbors() which does bidirectional traversal.
            for neighbor in self.graph.neighbors(current) {
                if neighbor == to {
                    return Some(dist + 1);
                }
                if !visited.contains_key(neighbor) {
                    visited.insert(neighbor, dist + 1);
                    queue.push_back((neighbor, dist + 1));
                }
            }
        }

        None
    }

    /// Map graph distance to detail level per the RAG strategy spec.
    ///
    /// | Distance | Detail |
    /// |----------|--------|
    /// | 0-1      | Full   |
    /// | 2        | Summary|
    /// | 3+       | NameOnly|
    pub fn detail_for_distance(&self, distance: usize) -> DetailLevel {
        match distance {
            0..=1 => DetailLevel::Full,
            2 => DetailLevel::Summary,
            _ => DetailLevel::NameOnly,
        }
    }

    /// Lore categories enriched by the given player intent.
    pub fn enrichment_categories(&self, intent: Intent) -> Vec<&'static str> {
        match intent {
            Intent::Combat => vec!["faction"],
            Intent::Dialogue => vec!["culture", "faction"],
            Intent::Exploration | Intent::Examine => vec!["location"],
            Intent::Chase => vec!["location", "faction"],
            Intent::Backstory => vec!["backstory"],
            Intent::Accusation => vec!["faction", "culture"],
            Intent::Meta => vec![],
            _ => vec![],
        }
    }

    /// Select lore entities for prompt injection.
    ///
    /// Returns every graph node as a `LoreSelection` at the appropriate detail
    /// level (closed-world assertion — all entities always present). NPCs in
    /// scene can upgrade their faction/culture to Full via `npc_presence` signal.
    pub fn select_lore(
        &self,
        current_node: &str,
        intent: Intent,
        npcs: &[NpcRegistryEntry],
        _arcs: &[String],
    ) -> Vec<LoreSelection> {
        let mut selections: HashMap<String, LoreSelection> = HashMap::new();

        // Layer 1: Graph-distance-based detail for every node (closed-world).
        for node in &self.graph.nodes {
            let dist = self.graph_distance(current_node, &node.id);
            let (detail, reason) = match dist {
                Some(d) => (self.detail_for_distance(d), format!("graph_distance_{}", d)),
                None => (DetailLevel::NameOnly, "unreachable".to_string()),
            };
            selections.insert(
                node.id.clone(),
                LoreSelection {
                    entity_id: node.id.clone(),
                    entity_name: node.name.clone(),
                    category: "location".to_string(),
                    detail_level: detail,
                    reason,
                },
            );
        }

        // Layer 2: NPC-presence enrichment — NPCs in scene pull their faction/culture.
        for npc in npcs {
            let npc_faction_id = format!("faction_{}", npc.role);
            // Upgrade or insert a faction entry at Full detail.
            let entry = selections.entry(npc_faction_id.clone()).or_insert_with(|| {
                LoreSelection {
                    entity_id: npc_faction_id.clone(),
                    entity_name: format!("{}'s faction", npc.name),
                    category: "faction".to_string(),
                    detail_level: DetailLevel::NameOnly,
                    reason: String::new(),
                }
            });
            entry.detail_level = DetailLevel::Full;
            entry.reason = format!("npc_presence:{}", npc.name);
        }

        selections.into_values().collect()
    }

    /// Format selections as a prompt section string for the narrator's Valley zone.
    ///
    /// Groups by detail level: Full → Summary → NameOnly (closed-world assertion).
    /// Returns empty string if no selections.
    pub fn format_prompt_section(selections: &[LoreSelection]) -> String {
        let mut content = String::new();

        let full: Vec<_> = selections.iter().filter(|s| s.detail_level == DetailLevel::Full).collect();
        let summary: Vec<_> = selections.iter().filter(|s| s.detail_level == DetailLevel::Summary).collect();
        let name_only: Vec<_> = selections.iter().filter(|s| s.detail_level == DetailLevel::NameOnly).collect();

        if !full.is_empty() {
            content.push_str("[NEARBY LORE — FULL DETAIL]\n");
            for s in &full {
                content.push_str(&format!("- {} ({}): {}\n", s.entity_name, s.category, s.entity_id));
            }
        }

        if !summary.is_empty() {
            content.push_str("\n[DISTANT LORE — SUMMARY ONLY]\n");
            for s in &summary {
                content.push_str(&format!("- {} ({})\n", s.entity_name, s.category));
            }
        }

        if !name_only.is_empty() {
            let names: Vec<_> = name_only.iter().map(|s| s.entity_name.as_str()).collect();
            content.push_str(&format!(
                "\n[KNOWN ENTITIES — NAMES ONLY (do not invent details)]\n{}\n",
                names.join(", ")
            ));
        }

        content
    }

    /// Format selections as a human-readable OTEL summary string.
    pub fn format_otel_summary(&self, selections: &[LoreSelection]) -> String {
        let mut included = Vec::new();
        let mut name_only = Vec::new();

        for s in selections {
            match s.detail_level {
                DetailLevel::Full | DetailLevel::Summary => {
                    included.push(format!("{} ({})", s.entity_name, s.detail_level_label()));
                }
                DetailLevel::NameOnly => {
                    name_only.push(s.entity_name.clone());
                }
            }
        }

        let mut parts = Vec::new();
        if !included.is_empty() {
            parts.push(format!("included: {}", included.join(", ")));
        }
        if !name_only.is_empty() {
            parts.push(format!("name_only: {}", name_only.join(", ")));
        }
        if parts.is_empty() {
            "excluded: all".to_string()
        } else {
            parts.join(". ")
        }
    }
}

impl LoreSelection {
    fn detail_level_label(&self) -> &'static str {
        match self.detail_level {
            DetailLevel::Full => "full",
            DetailLevel::Summary => "summary",
            DetailLevel::NameOnly => "name_only",
            _ => "unknown",
        }
    }
}
