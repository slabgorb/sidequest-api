# sidequest-namegen

CLI tool that generates complete NPC identity blocks from genre pack data.
Used by the agent system to create consistent, genre-appropriate NPCs.

## Usage

```bash
cargo run -p sidequest-namegen -- \
  --genre-packs-path ../sidequest-content/genre_packs \
  --genre neon_dystopia \
  --culture "the_synths" \
  --archetype "fixer" \
  --gender nonbinary
```

## What it generates

A complete NPC identity block:
- Culture-appropriate name (Markov chain from genre pack corpus)
- Pronouns, gender, faction, and archetype
- Appearance and personality description
- Dialogue quirks and history
- OCEAN personality profile with narrative summary
- Disposition, inventory hints, stat ranges
- Trope connections from genre pack

Output is JSON to stdout.
