# sidequest-validate — Feature Inventory

CLI binary that validates genre pack YAML schemas against Rust types. **~280 LOC,
single file (`main.rs`), fully complete.**

## COMPLETE — Do Not Rewrite

- **validate_genre_pack()** — loads every YAML file in a genre pack against expected
  Rust types. Reports ALL deserialization errors at once (not first-error stopping).
- **validate_world()** — validates a single world subdirectory.
- **check_yaml\<T\>()** — generic YAML deserializer with error collection.

## Validated Files

**Required (per genre pack):**
pack.yaml, rules.yaml, lore.yaml, theme.yaml, archetypes.yaml, char_creation.yaml,
visual_style.yaml, progression.yaml, axes.yaml, audio.yaml, cultures.yaml,
prompts.yaml, tropes.yaml

**Optional (per genre pack):**
achievements.yaml, power_tiers.yaml, beat_vocabulary.yaml, voice_presets.yaml,
pacing.yaml, inventory.yaml, openings.yaml

**Per world:** world.yaml, lore.yaml, cartography.yaml + optional overrides

**Per scenario:** scenario.yaml

## CLI Arguments

| Flag | Required | Description |
|------|----------|-------------|
| `--genre-packs-path` | Yes | Path to genre packs directory |
| `--genre` | No | Validate single genre (validates all if omitted) |

## Output

Exit code 0 if all validations pass. Prints per-file pass/fail with serde_yaml
error messages on failure.
