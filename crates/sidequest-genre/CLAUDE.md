# sidequest-genre — Feature Inventory

YAML genre pack loader and data models. **~3,600 LOC, fully complete.**

## COMPLETE — Do Not Rewrite

- **GenrePack** — `models.rs` (1,827 LOC) — all genre pack types. This is monolithic
  but well-organized. Includes: World, Scenario, TropeDefinition, NpcArchetype,
  CharCreationScene, DramaThresholds, GenreTheme, VisualStyle, AudioConfig,
  RulesConfig, Lore, PackMeta.
- **OceanProfile** — Big Five personality (0.0-10.0 per dimension). Has random(),
  behavioral_summary(), with_jitter(), apply_shift(). Canonical definition lives
  here (re-exported by sidequest-game to avoid circular deps).
- **GenreLoader** — `loader.rs` (272 LOC) — unified YAML directory loading with
  optional file handling (beat_vocabulary, achievements, voice_presets, power_tiers).
- **Markov chains** — `markov.rs` (291 LOC) — character-level Markov for word generation.
- **Name generator** — `names.rs` (312 LOC) — template-based with corpus blending.
- **Trope inheritance** — `resolve.rs` (161 LOC) — flattens trope inheritance chains.
- **Validation** — `validate.rs` (169 LOC) — genre pack validation rules.
- **LRU Cache** — `cache.rs` (48 LOC) — genre pack instance caching.

## Important Notes

- Genre packs live at `sidequest-content/genre_packs/`, NOT `oq-2/genre_packs/`
  (the root dir only has media subdirs).
- `#[serde(deny_unknown_fields)]` on key types — YAML typos will fail loudly.
- DramaThresholds controls pacing (sentence delivery, render threshold, escalation).
- Zero TODO/FIXME — this crate is done.
