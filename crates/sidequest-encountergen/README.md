# sidequest-encountergen

CLI tool that generates enemy stat blocks from genre pack data. Used by the
agent system to create mechanically sound, genre-appropriate adversaries.

## Usage

```bash
cargo run -p sidequest-encountergen -- \
  --genre-packs-path ../sidequest-content/genre_packs \
  --genre mutant_wasteland \
  --tier 2 \
  --count 3 \
  --culture "the_scorched"
```

## What it generates

Each enemy block includes:
- Culture-appropriate name (Markov chain from genre pack corpus)
- Class, race, level, and tier label
- HP, abilities (tier-scaled), and weaknesses
- OCEAN personality profile with jitter
- Disposition and dialogue quirks
- Inventory, stat scores, trope connections
- Visual prompt for image generation

Output is JSON to stdout — pipe to a file or consume from a subprocess.
