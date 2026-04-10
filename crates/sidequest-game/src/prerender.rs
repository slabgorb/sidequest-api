//! Speculative prerendering — queue image generation during voice playback.
//!
//! While TTS narration is playing (5–15 seconds), the [`PrerenderScheduler`]
//! predicts what the next scene image will be and submits a speculative render
//! job. If the prediction matches the actual next render request, the image
//! is already cached — zero perceived latency.

use crate::subject::{RenderSubject, SceneType, SubjectTier};

/// Configuration for speculative prerendering.
#[derive(Debug, Clone)]
pub struct PrerenderConfig {
    /// Whether speculation is enabled.
    pub enabled: bool,
    /// Maximum concurrent speculative render jobs (default 1).
    pub max_speculative_jobs: usize,
    /// Minimum hit rate before disabling speculation (default 0.3).
    pub min_hit_rate: f32,
}

impl Default for PrerenderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_speculative_jobs: 1,
            min_hit_rate: 0.3,
        }
    }
}

/// Game state context for subject prediction.
#[derive(Debug, Clone, Default)]
pub struct PrerenderContext {
    /// Whether the party is in active combat.
    pub in_combat: bool,
    /// Names of active combatants.
    pub combatant_names: Vec<String>,
    /// Destination the party is traveling to.
    pub pending_destination: Option<String>,
    /// NPC the party is currently in dialogue with.
    pub active_dialogue_npc: Option<String>,
    /// Genre art style for render requests.
    pub art_style: String,
    /// Negative prompt from genre visual_style for render requests.
    pub negative_prompt: String,
}

/// Tracks hit/miss rate for speculative renders.
#[derive(Debug, Clone, Default)]
pub struct WasteTracker {
    hits: u32,
    misses: u32,
    threshold: f32,
}

impl WasteTracker {
    /// Create a new tracker with the given minimum hit rate threshold.
    pub fn new(threshold: f32) -> Self {
        Self {
            hits: 0,
            misses: 0,
            threshold,
        }
    }

    /// Total speculative renders attempted.
    pub fn total(&self) -> u32 {
        self.hits + self.misses
    }

    /// Hit rate as a fraction (0.0–1.0).
    pub fn hit_rate(&self) -> f32 {
        if self.total() == 0 {
            return 0.0;
        }
        self.hits as f32 / self.total() as f32
    }

    /// Whether speculation should continue based on hit rate.
    ///
    /// Always continues for the first 10 samples (learning period).
    pub fn should_continue(&self) -> bool {
        self.total() < 10 || self.hit_rate() >= self.threshold
    }

    /// Record a hit (prerender matched actual render).
    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    /// Record a miss (prerender was wasted).
    pub fn record_miss(&mut self) {
        self.misses += 1;
    }
}

/// Schedules speculative image renders during TTS playback windows.
#[derive(Debug)]
pub struct PrerenderScheduler {
    config: PrerenderConfig,
    waste: WasteTracker,
    /// Content hash of the currently pending speculative render.
    pending_hash: Option<u64>,
}

impl PrerenderScheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(config: PrerenderConfig) -> Self {
        Self {
            waste: WasteTracker::new(config.min_hit_rate),
            config,
            pending_hash: None,
        }
    }

    /// Speculatively prerender based on game context.
    ///
    /// Returns a `RenderSubject` if conditions are met, or `None` if speculation
    /// is skipped. The caller should submit the returned subject to the render queue.
    pub fn speculate(&mut self, ctx: &PrerenderContext) -> Option<RenderSubject> {
        if !self.config.enabled {
            return None;
        }
        if !self.waste.should_continue() {
            tracing::debug!(
                hit_rate = self.waste.hit_rate(),
                total = self.waste.total(),
                "Speculative prerendering disabled due to low hit rate"
            );
            return None;
        }

        if self.pending_hash.is_some() {
            return None;
        }

        let subject = Self::predict_next_subject(ctx)?;

        let hash = crate::render_queue::compute_content_hash(&subject);
        self.pending_hash = Some(hash);

        Some(subject)
    }

    /// Record whether a render matched the speculative prerender.
    ///
    /// Call this when a real render completes. If `content_hash` matches
    /// the pending speculative hash, it's a hit.
    pub fn record_outcome(&mut self, content_hash: u64) {
        if let Some(pending) = self.pending_hash.take() {
            if content_hash == pending {
                self.waste.record_hit();
            } else {
                self.waste.record_miss();
            }
        }
    }

    /// Access the waste tracker for monitoring.
    pub fn waste_tracker(&self) -> &WasteTracker {
        &self.waste
    }

    /// Predict the most likely next render subject based on game state.
    fn predict_next_subject(ctx: &PrerenderContext) -> Option<RenderSubject> {
        // Strategy 1: Combat → predict combat scene
        if ctx.in_combat && !ctx.combatant_names.is_empty() {
            return RenderSubject::new(
                ctx.combatant_names.clone(),
                SceneType::Combat,
                SubjectTier::Scene,
                format!("combat scene with {}", ctx.combatant_names.join(", ")),
                0.8,
            );
        }

        // Strategy 2: Pending destination → predict landscape
        if let Some(ref dest) = ctx.pending_destination {
            return RenderSubject::new(
                vec![],
                SceneType::Exploration,
                SubjectTier::Landscape,
                format!("view of {}", dest),
                0.7,
            );
        }

        // Strategy 3: Active dialogue → predict portrait
        if let Some(ref npc) = ctx.active_dialogue_npc {
            return RenderSubject::new(
                vec![npc.clone()],
                SceneType::Dialogue,
                SubjectTier::Portrait,
                format!("portrait of {}", npc),
                0.6,
            );
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_prediction() {
        let mut scheduler = PrerenderScheduler::new(PrerenderConfig::default());
        let ctx = PrerenderContext {
            in_combat: true,
            combatant_names: vec!["Dragon".to_string(), "Knight".to_string()],
            ..Default::default()
        };
        let subject = scheduler.speculate(&ctx).unwrap();
        assert_eq!(*subject.scene_type(), SceneType::Combat);
        assert_eq!(*subject.tier(), SubjectTier::Scene);
    }

    #[test]
    fn location_prediction() {
        let mut scheduler = PrerenderScheduler::new(PrerenderConfig::default());
        let ctx = PrerenderContext {
            pending_destination: Some("Crystal Caverns".to_string()),
            ..Default::default()
        };
        let subject = scheduler.speculate(&ctx).unwrap();
        assert_eq!(*subject.scene_type(), SceneType::Exploration);
        assert_eq!(*subject.tier(), SubjectTier::Landscape);
    }

    #[test]
    fn dialogue_prediction() {
        let mut scheduler = PrerenderScheduler::new(PrerenderConfig::default());
        let ctx = PrerenderContext {
            active_dialogue_npc: Some("Elara".to_string()),
            ..Default::default()
        };
        let subject = scheduler.speculate(&ctx).unwrap();
        assert_eq!(*subject.scene_type(), SceneType::Dialogue);
        assert_eq!(*subject.tier(), SubjectTier::Portrait);
    }

    #[test]
    fn dedup_integration() {
        let mut scheduler = PrerenderScheduler::new(PrerenderConfig::default());
        let ctx = PrerenderContext {
            in_combat: true,
            combatant_names: vec!["Goblin".to_string()],
            ..Default::default()
        };

        let subject = scheduler.speculate(&ctx).unwrap();
        let hash = crate::render_queue::compute_content_hash(&subject);

        scheduler.record_outcome(hash);
        assert_eq!(scheduler.waste_tracker().hits, 1);
    }

    #[test]
    fn waste_tracking_disables() {
        let config = PrerenderConfig {
            min_hit_rate: 0.5,
            ..Default::default()
        };
        let mut scheduler = PrerenderScheduler::new(config);

        for _ in 0..11 {
            scheduler.waste.record_miss();
        }

        let ctx = PrerenderContext {
            in_combat: true,
            combatant_names: vec!["Goblin".to_string()],
            ..Default::default()
        };
        let result = scheduler.speculate(&ctx);
        assert!(result.is_none(), "Should disable after sustained low hit rate");
    }

    #[test]
    fn configurable_disable() {
        let config = PrerenderConfig {
            enabled: false,
            ..Default::default()
        };
        let mut scheduler = PrerenderScheduler::new(config);
        let ctx = PrerenderContext {
            in_combat: true,
            combatant_names: vec!["Goblin".to_string()],
            ..Default::default()
        };
        assert!(scheduler.speculate(&ctx).is_none());
    }

    #[test]
    fn graceful_noop_no_prediction() {
        let mut scheduler = PrerenderScheduler::new(PrerenderConfig::default());
        let ctx = PrerenderContext::default();
        let result = scheduler.speculate(&ctx);
        assert!(result.is_none(), "No prediction possible should return None gracefully");
    }

    #[test]
    fn waste_tracker_hit_rate() {
        let mut tracker = WasteTracker::new(0.3);
        tracker.record_hit();
        tracker.record_hit();
        tracker.record_miss();
        assert!((tracker.hit_rate() - 0.6667).abs() < 0.01);
        assert!(tracker.should_continue());
    }
}
