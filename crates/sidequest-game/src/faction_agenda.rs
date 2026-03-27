//! Faction agenda model — schema for faction goals, urgency, and scene
//! injection rules.
//!
//! Story 6-4: Defines the `FactionAgenda` type that tracks what factions
//! want and how urgently they want it. Non-dormant agendas produce scene
//! injection text for the narrator prompt (wired in story 6-5).

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::scene_directive::DirectivePriority;

/// How urgently a faction is pursuing its agenda.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AgendaUrgency {
    /// Not currently relevant — no scene injection.
    #[default]
    Dormant,
    /// Background tension — low-priority scene flavor.
    Simmering,
    /// Should influence upcoming scenes — medium priority.
    Pressing,
    /// Must influence the next scene — high priority.
    Critical,
}

impl AgendaUrgency {
    /// Map urgency to a scene directive priority. Dormant returns None
    /// because dormant agendas should not produce directive elements.
    pub fn to_directive_priority(self) -> Option<DirectivePriority> {
        match self {
            Self::Dormant => None,
            Self::Simmering => Some(DirectivePriority::Low),
            Self::Pressing => Some(DirectivePriority::Medium),
            Self::Critical => Some(DirectivePriority::High),
        }
    }
}

/// Error type for faction agenda validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FactionAgendaError {
    /// Faction name cannot be empty or whitespace-only.
    #[error("faction name cannot be empty or whitespace-only")]
    EmptyFactionName,
    /// Goal cannot be empty or whitespace-only.
    #[error("goal cannot be empty or whitespace-only")]
    EmptyGoal,
    /// Event text cannot be empty or whitespace-only.
    #[error("event text cannot be empty or whitespace-only")]
    EmptyEventText,
}

/// A faction's agenda: what they want, how badly, and what text to inject.
#[derive(Debug, Clone)]
pub struct FactionAgenda {
    faction_name: String,
    goal: String,
    urgency: AgendaUrgency,
    event_text: String,
}

impl FactionAgenda {
    /// Validated constructor. All text fields must be non-empty and not
    /// whitespace-only.
    pub fn try_new(
        faction_name: String,
        goal: String,
        urgency: AgendaUrgency,
        event_text: String,
    ) -> Result<Self, FactionAgendaError> {
        if faction_name.trim().is_empty() {
            warn!("FactionAgenda rejected: empty faction name");
            return Err(FactionAgendaError::EmptyFactionName);
        }
        if goal.trim().is_empty() {
            warn!("FactionAgenda rejected: empty goal");
            return Err(FactionAgendaError::EmptyGoal);
        }
        if event_text.trim().is_empty() {
            warn!("FactionAgenda rejected: empty event text");
            return Err(FactionAgendaError::EmptyEventText);
        }
        Ok(Self {
            faction_name,
            goal,
            urgency,
            event_text,
        })
    }

    /// The faction that owns this agenda.
    pub fn faction_name(&self) -> &str {
        &self.faction_name
    }

    /// What the faction wants.
    pub fn goal(&self) -> &str {
        &self.goal
    }

    /// How urgently the faction is pursuing this goal.
    pub fn urgency(&self) -> AgendaUrgency {
        self.urgency
    }

    /// The narrative text template for scene injection.
    pub fn event_text(&self) -> &str {
        &self.event_text
    }

    /// Update the urgency level (escalation or de-escalation).
    pub fn set_urgency(&mut self, urgency: AgendaUrgency) {
        self.urgency = urgency;
    }

    /// Produce the scene injection text, if the agenda is active (non-dormant).
    /// Returns `None` for dormant agendas.
    pub fn scene_injection(&self) -> Option<String> {
        if self.urgency == AgendaUrgency::Dormant {
            return None;
        }
        Some(self.event_text.clone())
    }
}

/// Raw deserialization target for `#[serde(try_from)]`.
#[derive(Deserialize)]
struct RawFactionAgenda {
    faction_name: String,
    goal: String,
    urgency: AgendaUrgency,
    event_text: String,
}

impl TryFrom<RawFactionAgenda> for FactionAgenda {
    type Error = FactionAgendaError;

    fn try_from(raw: RawFactionAgenda) -> Result<Self, Self::Error> {
        Self::try_new(raw.faction_name, raw.goal, raw.urgency, raw.event_text)
    }
}

impl<'de> Deserialize<'de> for FactionAgenda {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawFactionAgenda::deserialize(deserializer)?;
        FactionAgenda::try_from(raw).map_err(serde::de::Error::custom)
    }
}
