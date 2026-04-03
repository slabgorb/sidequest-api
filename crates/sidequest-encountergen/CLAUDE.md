# sidequest-encountergen — Feature Inventory

CLI binary that generates enemy stat blocks from genre pack data. **~550 LOC,
single file (`main.rs`), fully complete.**

## COMPLETE — Do Not Rewrite

- **generate_enemy()** — main generation: culture-appropriate names via Markov
  chains, class/archetype stats, HP, tier-scaled abilities, weaknesses.
- **generate_abilities()** — class-based ability selection with tier scaling (1-4).
- **generate_weaknesses()** — class/race-based weakness derivation.
- **jitter_ocean()** — OCEAN personality with random jitter from archetype baseline.
- **match_tropes()** — tag-based trope connection matching from genre pack.
- **build_visual_prompt()** — combines power tier NPC description + archetype +
  context + genre visual style for image generation.

## CLI Arguments

| Flag | Required | Description |
|------|----------|-------------|
| `--genre-packs-path` | Yes | Path to genre packs directory |
| `--genre` | Yes | Genre code (e.g. `mutant_wasteland`) |
| `--tier` | No | Power tier 1-4 |
| `--count` | No | Number of enemies (default 1) |
| `--role` | No | Flavors stat block |
| `--class` | No | Defaults to random |
| `--culture` | No | For name generation |
| `--archetype` | No | NPC archetype |
| `--context` | No | For visual prompt |

## Output

JSON `EncounterBlock` containing `Vec<EnemyBlock>`. Each enemy has: name, class,
race, level, tier_label, role, hp, abilities, weaknesses, disposition,
personality, dialogue_quirks, inventory, stat_scores, OCEAN values,
trope_connections, visual_prompt.
