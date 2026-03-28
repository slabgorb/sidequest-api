//! Campaign maturity and world materialization (Story 6-6).
//!
//! Determines campaign maturity from turn count and story beats fired,
//! then bootstraps the GameSnapshot with appropriate history chapters
//! from the genre pack.

use serde::{Deserialize, Serialize};

use crate::state::GameSnapshot;

/// Campaign maturity level derived from game progression.
///
/// Maturity controls which history chapters are applied to the GameSnapshot,
/// letting fresh campaigns feel sparse and veteran campaigns feel rich.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CampaignMaturity {
    /// Turns 0-5 effective: minimal history, world is new.
    Fresh,
    /// Turns 6-20 effective: factions introduced, stakes emerging.
    Early,
    /// Turns 21-50 effective: established relationships, escalating tensions.
    Mid,
    /// Turns 51+ effective: deep history, faction conflicts in motion.
    Veteran,
}

impl Default for CampaignMaturity {
    fn default() -> Self {
        Self::Fresh
    }
}

impl CampaignMaturity {
    /// Derive maturity from a game snapshot's turn count and beats fired.
    ///
    /// Beats accelerate maturity — a dramatic early game matures faster.
    /// Uses saturating arithmetic to prevent overflow with large beat counts.
    pub fn from_snapshot(snapshot: &GameSnapshot) -> Self {
        let turn = snapshot.turn_manager.round();
        let beats = snapshot.total_beats_fired;
        let effective_turns = (turn as u64).saturating_add((beats / 2) as u64);
        match effective_turns {
            0..=5 => Self::Fresh,
            6..=20 => Self::Early,
            21..=50 => Self::Mid,
            _ => Self::Veteran,
        }
    }

    /// Map a chapter id string to the corresponding maturity level.
    fn from_chapter_id(id: &str) -> Option<Self> {
        match id {
            "fresh" => Some(Self::Fresh),
            "early" => Some(Self::Early),
            "mid" => Some(Self::Mid),
            "veteran" => Some(Self::Veteran),
            _ => None,
        }
    }
}

/// A history chapter from the genre pack, keyed by maturity level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryChapter {
    /// Maturity level key (fresh, early, mid, veteran).
    pub id: String,
    /// Human-readable chapter title.
    pub label: String,
    /// Lore fragments for this chapter.
    #[serde(default)]
    pub lore: Vec<String>,
}

/// Apply history chapters to a GameSnapshot based on campaign maturity.
///
/// Calculates maturity from the snapshot, then includes all chapters whose
/// maturity level is at or below the current level. Idempotent — replaces
/// existing world_history and campaign_maturity on each call.
pub fn materialize_world(snapshot: &mut GameSnapshot, chapters: &[HistoryChapter]) {
    let maturity = CampaignMaturity::from_snapshot(snapshot);
    let applicable: Vec<HistoryChapter> = chapters
        .iter()
        .filter(|ch| {
            CampaignMaturity::from_chapter_id(&ch.id)
                .map(|ch_maturity| ch_maturity <= maturity)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    snapshot.world_history = applicable;
    snapshot.campaign_maturity = maturity;
}
