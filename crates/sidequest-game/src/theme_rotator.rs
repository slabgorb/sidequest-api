//! Theme rotation — anti-repetition track selection within mood categories.
//!
//! Ensures audio variety by tracking recently played tracks per mood and selecting
//! the least-recently-used option, with energy-based preference scoring.

use std::collections::{HashMap, VecDeque};

use rand::seq::IndexedRandom;
use rand::SeedableRng;
use rand::rngs::StdRng;
use sidequest_genre::MoodTrack;

/// Configuration for theme rotation behaviour.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// How many recent tracks to exclude from selection per mood (default 3).
    pub history_depth: usize,
    /// Whether to randomise among eligible tracks (default true).
    pub randomize: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            history_depth: 3,
            randomize: true,
        }
    }
}

/// Anti-repetition track selector.
///
/// Tracks play history per mood category (keyed by string, e.g. "combat")
/// and filters recently played tracks before selecting.
pub struct ThemeRotator {
    config: RotationConfig,
    /// Per-mood play history (mood key → recent track titles).
    history: HashMap<String, VecDeque<String>>,
    rng: StdRng,
}

impl ThemeRotator {
    /// Create a new rotator with the given configuration.
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config,
            history: HashMap::new(),
            rng: StdRng::from_os_rng(),
        }
    }

    /// Create a rotator with a seeded RNG for deterministic testing.
    #[cfg(test)]
    pub fn new_seeded(config: RotationConfig, seed: u64) -> Self {
        Self {
            config,
            history: HashMap::new(),
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Select a track for the given mood, avoiding recently played tracks.
    ///
    /// Returns `None` if `tracks` is empty.
    ///
    /// When multiple tracks are eligible, prefers tracks whose energy level
    /// is closest to the requested `intensity`.
    pub fn select<'a>(
        &mut self,
        mood_key: &str,
        tracks: &'a [MoodTrack],
        intensity: f32,
    ) -> Option<&'a MoodTrack> {
        if tracks.is_empty() {
            return None;
        }

        let history = self
            .history
            .entry(mood_key.to_string())
            .or_insert_with(VecDeque::new);

        // Filter out recently played tracks
        let mut eligible: Vec<(usize, &MoodTrack)> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !history.contains(&t.title))
            .collect();

        // If all tracks exhausted, reset history and allow all
        if eligible.is_empty() {
            history.clear();
            eligible = tracks.iter().enumerate().collect();
        }

        // Score by energy match (lower distance = better match)
        eligible.sort_by(|a, b| {
            let da = (a.1.energy as f32 - intensity).abs();
            let db = (b.1.energy as f32 - intensity).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        let selected = if self.config.randomize && eligible.len() > 1 {
            // Pick randomly from the top candidates (up to 3 best energy matches)
            let top_n = eligible.len().min(3);
            let candidates = &eligible[..top_n];
            let idx = (0..candidates.len()).collect::<Vec<_>>();
            let &pick = idx.choose(&mut self.rng)?;
            candidates[pick].1
        } else {
            // Deterministic: best energy match
            eligible[0].1
        };

        // Record in history
        history.push_back(selected.title.clone());
        if history.len() > self.config.history_depth {
            history.pop_front();
        }

        Some(selected)
    }

    /// Return a snapshot of per-mood play history for telemetry.
    pub fn history_snapshot(&self) -> HashMap<String, Vec<String>> {
        self.history.iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect()
    }
}

impl std::fmt::Debug for ThemeRotator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeRotator")
            .field("config", &self.config)
            .field("history", &self.history)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, energy: f64) -> MoodTrack {
        MoodTrack {
            path: format!("audio/music/{}.ogg", title),
            title: title.to_string(),
            bpm: 120,
            energy,
        }
    }

    #[test]
    fn no_immediate_repeat() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![
            track("battle_1", 0.5),
            track("battle_2", 0.5),
            track("battle_3", 0.5),
        ];

        let first = rotator.select("combat", &tracks, 0.5).unwrap();
        let first_title = first.title.clone();

        let second = rotator.select("combat", &tracks, 0.5).unwrap();
        assert_ne!(
            second.title, first_title,
            "Should not immediately repeat the same track"
        );
    }

    #[test]
    fn history_depth_respected() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![
            track("a", 0.5),
            track("b", 0.5),
            track("c", 0.5),
            track("d", 0.5),
            track("e", 0.5),
        ];

        // Play 4 tracks — first should be evicted from history (depth=3)
        let t1 = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();
        let _t2 = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();
        let _t3 = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();
        let _t4 = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();

        // t1 should be eligible again (evicted after 3 newer entries)
        let history = rotator.history.get("mood").unwrap();
        assert!(
            !history.contains(&t1),
            "Track played 4+ ago should be evicted from history"
        );
    }

    #[test]
    fn exhaustion_reset() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![track("x", 0.5), track("y", 0.5), track("z", 0.5)];

        // Play all 3 tracks
        rotator.select("combat", &tracks, 0.5);
        rotator.select("combat", &tracks, 0.5);
        rotator.select("combat", &tracks, 0.5);

        // 4th selection should reset and succeed
        let result = rotator.select("combat", &tracks, 0.5);
        assert!(result.is_some(), "Should reset history when all exhausted");
    }

    #[test]
    fn single_track_always_returns() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![track("only_one", 0.5)];

        let first = rotator.select("mood", &tracks, 0.5).unwrap();
        assert_eq!(first.title, "only_one");

        // Second call triggers exhaustion reset and returns the same track
        let second = rotator.select("mood", &tracks, 0.5).unwrap();
        assert_eq!(second.title, "only_one");
    }

    #[test]
    fn empty_tracks_returns_none() {
        let config = RotationConfig::default();
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks: Vec<MoodTrack> = vec![];
        assert!(rotator.select("mood", &tracks, 0.5).is_none());
    }

    #[test]
    fn energy_preference() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![
            track("low_energy", 0.2),
            track("high_energy", 0.9),
            track("mid_energy", 0.5),
        ];

        // High intensity should prefer high-energy track
        let selected = rotator.select("combat", &tracks, 0.9).unwrap();
        assert_eq!(selected.title, "high_energy");
    }

    #[test]
    fn per_mood_isolation() {
        let config = RotationConfig {
            history_depth: 3,
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let combat_tracks = vec![track("battle", 0.8)];
        let explore_tracks = vec![track("wander", 0.3)];

        rotator.select("combat", &combat_tracks, 0.8);
        // Exploration history is independent — "wander" should be available
        let result = rotator.select("exploration", &explore_tracks, 0.3);
        assert!(result.is_some());
        assert_eq!(result.unwrap().title, "wander");
    }

    #[test]
    fn random_mode_varies_selection() {
        let config = RotationConfig {
            history_depth: 1,
            randomize: true,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![
            track("a", 0.5),
            track("b", 0.5),
            track("c", 0.5),
        ];

        // With randomize=true and multiple equal-energy tracks, we should get variety
        let mut seen = std::collections::HashSet::new();
        for _ in 0..20 {
            let t = rotator.select("mood", &tracks, 0.5).unwrap();
            seen.insert(t.title.clone());
        }
        assert!(
            seen.len() > 1,
            "Random mode should produce variety: got {:?}",
            seen
        );
    }

    #[test]
    fn deterministic_mode_consistent() {
        let config = RotationConfig {
            history_depth: 0, // No history exclusion
            randomize: false,
        };
        let mut rotator = ThemeRotator::new_seeded(config, 42);
        let tracks = vec![
            track("best", 0.5),
            track("ok", 0.3),
            track("meh", 0.1),
        ];

        // With randomize=false, always picks best energy match
        let first = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();
        let second = rotator.select("mood", &tracks, 0.5).unwrap().title.clone();
        assert_eq!(first, second, "Deterministic mode should be consistent");
        assert_eq!(first, "best");
    }
}
