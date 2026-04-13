# sidequest-genre

YAML genre pack loader, models, and validation.

Genre packs are swappable narrative configurations — each one defines a complete
game world with lore, rules, character creation, tropes, audio, and visual style.

## What's in here

- **`load_genre_pack(path)`** — Load and parse a genre pack directory
- **`GenreCache`** — Cache loaded packs to avoid re-parsing
- **`GenreCode`** — Validated genre identifier newtype
- **Models** — Strongly-typed structs for every YAML file in a genre pack. 13 are required (`pack`, `rules`, `lore`, `theme`, `archetypes`, `char_creation`, `visual_style`, `progression`, `axes`, `audio`, `cultures`, `prompts`, `tropes`) and up to 9 are optional (`achievements`, `power_tiers`, `beat_vocabulary`, `voice_presets`, `pacing`, `inventory`, `openings`, `backstory_tables`, `equipment_tables`). See `loader.rs` for the canonical list. `OceanProfile` (Big Five personality) lives here and is re-exported by `sidequest-game`.
- **Validation** — Schema and cross-reference validation
- **Trope inheritance** — `resolve_trope_inheritance()` merges parent/child trope definitions
- **Name generation** — Template-based names with Markov chain corpus blending

## Usage

```rust
use std::path::Path;
let pack = sidequest_genre::load_genre_pack(Path::new("genre_packs/mutant_wasteland")).unwrap();
pack.validate().unwrap();
```

Genre pack format is defined by the types in `models.rs` — treat that file as
authoritative. The original Python prototype at `slabgorb/sq-2` is archived and
should not be consulted for format questions. Architectural context lives in
**ADR-003** (pack architecture) and **ADR-004** (lazy binding), in the
orchestrator repo at `orc-quest/docs/adr/`.
