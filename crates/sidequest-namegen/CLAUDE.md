# sidequest-namegen — Feature Inventory

CLI binary that generates complete NPC identity blocks from genre pack data.
**~310 LOC, single file (`main.rs`), fully complete.**

## COMPLETE — Do Not Rewrite

- **generate_npc()** — main generation: culture-appropriate name via Markov
  chains, archetype personality, OCEAN profile, dialogue quirks, faction context.
- **jitter_ocean()** — OCEAN personality with random jitter from archetype baseline.
- **summarize_ocean()** — converts numeric OCEAN to narrative summary.
- **generate_history()** — template-based history from archetype and faction context.
  6 fixed templates, 10 event templates, 6 deed templates.
- **match_tropes()** — tag-based trope connection matching.

## CLI Arguments

| Flag | Required | Description |
|------|----------|-------------|
| `--genre-packs-path` | Yes | Path to genre packs directory |
| `--genre` | Yes | Genre code |
| `--culture` | No | Random if omitted |
| `--archetype` | No | Random if omitted |
| `--gender` | No | male/female/nonbinary, random if omitted |
| `--role` | No | Defaults to archetype name |
| `--description` | No | Physical description hints |

## Output

JSON `NpcBlock`: name, pronouns, gender, culture, faction, faction_description,
archetype, role, appearance, personality, dialogue_quirks, history, OCEAN values,
ocean_summary, disposition, inventory, stat_ranges, trope_connections.
