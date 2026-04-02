# sidequest-loadoutgen — Feature Inventory

CLI binary that generates starting equipment sets from genre pack inventory
catalogs. **~230 LOC, single file (`main.rs`), fully complete.**

## COMPLETE — Do Not Rewrite

- **generate_loadout()** — resolves items from genre pack's `inventory.yaml`
  based on class and tier. Tier 1: base equipment only. Tier 2-4: adds 1-2
  bonus items from catalog with power_level > 1.
- **build_narrative_hook()** — generates a one-sentence intro for the loadout
  in-fiction (e.g. "You carry the standard kit of a wasteland scavenger...").

## CLI Arguments

| Flag | Required | Description |
|------|----------|-------------|
| `--genre-packs-path` | Yes | Path to genre packs directory |
| `--genre` | Yes | Genre code |
| `--class` | Yes | Matched against `starting_equipment` keys |
| `--tier` | No | Power tier 1-4 (default 1) |

## Output

JSON `LoadoutBlock`: class, currency_name, starting_gold, equipment
(`Vec<LoadoutItem>`), narrative_hook, total_value. Each item has: id, name,
description, category, value, tags, lore.
