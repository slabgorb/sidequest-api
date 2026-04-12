//! Beat filter — suppress image renders for low-narrative-weight actions.
//!
//! Evaluates narrative weight, render cooldown, burst rate limiting, and
//! duplicate subject suppression to decide whether a narration moment
//! deserves a rendered image. Configurable thresholds per genre pack.
//!
//! Story 4-3: Beat filter — suppress image renders for low-narrative-weight
//! actions, configurable thresholds.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::subject::RenderSubject;

/// Filter decision with auditable reasoning.
///
/// Returned by `BeatFilter::evaluate()` to indicate whether the render
/// pipeline should proceed or suppress this narration moment.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterDecision {
    /// Render this moment — it passes all suppression checks.
    Render {
        /// Human-readable explanation of why this moment qualifies.
        reason: String,
    },
    /// Suppress this moment — do not queue a render.
    Suppress {
        /// Human-readable explanation of which rule suppressed it.
        reason: String,
    },
}

impl FilterDecision {
    /// Returns `true` if the decision is to render.
    pub fn should_render(&self) -> bool {
        matches!(self, FilterDecision::Render { .. })
    }

    /// Returns the reason string regardless of decision variant.
    pub fn reason(&self) -> &str {
        match self {
            FilterDecision::Render { reason } | FilterDecision::Suppress { reason } => reason,
        }
    }
}

/// Configuration for the beat filter, loaded from genre pack YAML.
///
/// All fields are private with getters. Use `BeatFilterConfig::new()` for
/// validated construction or `BeatFilterConfig::default()` for sane defaults.
#[derive(Debug, Clone)]
pub struct BeatFilterConfig {
    weight_threshold: f32,
    cooldown: Duration,
    combat_threshold: f32,
    max_history: usize,
    burst_limit: u32,
    burst_window: Duration,
    /// How long a subject hash stays "active" for duplicate suppression.
    /// After this duration, the same subject can be rendered again.
    /// Default: 5 minutes. Prevents permanent suppression when staying
    /// in one location for extended gameplay.
    dedup_window: Duration,
}

impl BeatFilterConfig {
    /// Create a new config with validated values.
    ///
    /// Returns `None` if:
    /// - `weight_threshold` is outside \[0.0, 1.0\]
    /// - `combat_threshold` is outside \[0.0, 1.0\]
    /// - `combat_threshold` > `weight_threshold`
    /// - `max_history` is 0
    /// - `burst_limit` is 0
    pub fn new(
        weight_threshold: f32,
        cooldown: Duration,
        combat_threshold: f32,
        max_history: usize,
        burst_limit: u32,
        burst_window: Duration,
    ) -> Option<Self> {
        if !(0.0..=1.0).contains(&weight_threshold) {
            return None;
        }
        if !(0.0..=1.0).contains(&combat_threshold) {
            return None;
        }
        if combat_threshold > weight_threshold {
            return None;
        }
        if max_history == 0 {
            return None;
        }
        if burst_limit == 0 {
            return None;
        }
        Some(Self {
            weight_threshold,
            cooldown,
            combat_threshold,
            max_history,
            burst_limit,
            burst_window,
            dedup_window: Duration::from_secs(300),
        })
    }

    /// Minimum narrative weight to trigger a render (normal mode).
    pub fn weight_threshold(&self) -> f32 {
        self.weight_threshold
    }

    /// Minimum time between consecutive renders.
    pub fn cooldown(&self) -> Duration {
        self.cooldown
    }

    /// Lower weight threshold used during combat encounters.
    pub fn combat_threshold(&self) -> f32 {
        self.combat_threshold
    }

    /// Maximum number of render records kept in the rolling history.
    pub fn max_history(&self) -> usize {
        self.max_history
    }

    /// Maximum number of renders allowed within the burst window.
    pub fn burst_limit(&self) -> u32 {
        self.burst_limit
    }

    /// Time window for burst rate limiting.
    pub fn burst_window(&self) -> Duration {
        self.burst_window
    }
}

impl Default for BeatFilterConfig {
    fn default() -> Self {
        Self {
            weight_threshold: 0.4,
            cooldown: Duration::from_secs(15),
            combat_threshold: 0.25,
            max_history: 20,
            burst_limit: 5,
            burst_window: Duration::from_secs(120),
            dedup_window: Duration::from_secs(300),
        }
    }
}

/// Record of a past render for cooldown and burst tracking.
#[derive(Debug, Clone)]
struct RenderRecord {
    timestamp: Instant,
    subject_hash: u64,
    #[allow(dead_code)]
    narrative_weight: f32,
}

/// Contextual state that influences filter decisions.
///
/// Provided alongside the `RenderSubject` to `BeatFilter::evaluate()`.
#[derive(Debug, Clone, Default)]
pub struct FilterContext {
    /// Whether the game is currently in an active combat encounter.
    pub in_combat: bool,
    /// Whether this narration represents a scene transition.
    pub scene_transition: bool,
    /// Whether the player explicitly requested visual rendering.
    pub player_requested: bool,
}

/// Stateful beat filter that gates the image render pipeline.
///
/// Tracks render history for cooldown, burst limiting, and duplicate
/// subject suppression. Configurable per genre pack via `BeatFilterConfig`.
pub struct BeatFilter {
    config: BeatFilterConfig,
    render_history: VecDeque<RenderRecord>,
}

impl BeatFilter {
    /// Create a new beat filter with the given configuration.
    pub fn new(config: BeatFilterConfig) -> Self {
        Self {
            config,
            render_history: VecDeque::new(),
        }
    }

    /// Create a beat filter with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(BeatFilterConfig::default())
    }

    /// Evaluate whether a render subject should be rendered or suppressed.
    ///
    /// Checks (in order):
    /// 1. Force-render bypass (scene transition, player request)
    /// 2. Weight threshold (combat-aware)
    /// 3. Cooldown timer
    /// 4. Burst rate limit
    /// 5. Duplicate subject suppression (time-bounded by dedup_window)
    #[tracing::instrument(name = "beat_filter.evaluate", skip_all, fields(
        weight = subject.narrative_weight(),
        in_combat = context.in_combat,
        scene_transition = context.scene_transition,
        history_len = self.render_history.len(),
    ))]
    pub fn evaluate(&mut self, subject: &RenderSubject, context: &FilterContext) -> FilterDecision {
        let decision = self.evaluate_inner(subject, context);

        // OTEL: beat_filter.evaluated — GM panel verification that the filter
        // is engaged on every render decision. Emits post-decision so
        // `history_len` reflects whether this decision was recorded.
        let (decision_str, reason) = match &decision {
            FilterDecision::Render { reason } => ("render", reason.as_str()),
            FilterDecision::Suppress { reason } => ("suppress", reason.as_str()),
        };
        WatcherEventBuilder::new("beat_filter", WatcherEventType::StateTransition)
            .field("action", "beat_filter_evaluated")
            .field("decision", decision_str)
            .field("reason", reason)
            .field("subject_weight", subject.narrative_weight())
            .field("in_combat", context.in_combat)
            .field("scene_transition", context.scene_transition)
            .field("player_requested", context.player_requested)
            .field("history_len", self.render_history.len())
            .send();

        decision
    }

    /// Pure decision logic — no telemetry, just the filter rules.
    fn evaluate_inner(
        &mut self,
        subject: &RenderSubject,
        context: &FilterContext,
    ) -> FilterDecision {
        let now = Instant::now();

        // 1. Force-render bypass (scene transition, player request)
        if context.scene_transition {
            self.record_render(now, subject);
            return FilterDecision::Render {
                reason: "forced: scene transition".into(),
            };
        }
        if context.player_requested {
            self.record_render(now, subject);
            return FilterDecision::Render {
                reason: "forced: player requested".into(),
            };
        }

        // 2. Weight threshold (combat-aware)
        let threshold = if context.in_combat {
            self.config.combat_threshold
        } else {
            self.config.weight_threshold
        };
        if subject.narrative_weight() < threshold {
            return FilterDecision::Suppress {
                reason: format!(
                    "weight {:.2} below threshold {:.2}",
                    subject.narrative_weight(),
                    threshold
                ),
            };
        }

        // 3. Cooldown timer
        if self.config.cooldown > Duration::ZERO {
            if let Some(last) = self.render_history.back() {
                if now.duration_since(last.timestamp) < self.config.cooldown {
                    return FilterDecision::Suppress {
                        reason: "cooldown active".into(),
                    };
                }
            }
        }

        // 4. Burst rate limit
        let burst_count = self
            .render_history
            .iter()
            .filter(|r| now.duration_since(r.timestamp) < self.config.burst_window)
            .count() as u32;
        if burst_count >= self.config.burst_limit {
            return FilterDecision::Suppress {
                reason: format!("burst limit {} reached in window", self.config.burst_limit),
            };
        }

        // 5. Duplicate subject suppression (time-bounded)
        // Only suppress if the same subject was rendered within the dedup window.
        // Without this time bound, subjects rendered early in a session would
        // permanently block re-renders at the same location — the root cause of
        // "images stop generating after turn 2."
        let subject_hash = hash_subject(subject);
        if self.render_history.iter().any(|r| {
            r.subject_hash == subject_hash
                && now.duration_since(r.timestamp) < self.config.dedup_window
        }) {
            return FilterDecision::Suppress {
                reason: format!(
                    "duplicate subject in history (dedup window {}s)",
                    self.config.dedup_window.as_secs()
                ),
            };
        }

        // All checks passed — render
        self.record_render(now, subject);
        FilterDecision::Render {
            reason: format!(
                "weight {:.2} passed threshold {:.2}",
                subject.narrative_weight(),
                threshold
            ),
        }
    }

    /// Record a render in history and prune if needed.
    fn record_render(&mut self, timestamp: Instant, subject: &RenderSubject) {
        self.render_history.push_back(RenderRecord {
            timestamp,
            subject_hash: hash_subject(subject),
            narrative_weight: subject.narrative_weight(),
        });
        while self.render_history.len() > self.config.max_history {
            self.render_history.pop_front();
        }
    }

    /// Number of render records currently in history.
    pub fn history_len(&self) -> usize {
        self.render_history.len()
    }

    /// Clear all render history.
    pub fn clear_history(&mut self) {
        self.render_history.clear();
    }
}

/// Compute a content hash for a render subject (for dedup).
pub fn hash_subject(subject: &RenderSubject) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    subject.prompt_fragment().hash(&mut hasher);
    for entity in subject.entities() {
        entity.hash(&mut hasher);
    }
    hasher.finish()
}
