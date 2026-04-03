# sidequest-loadoutgen

CLI tool that generates starting equipment sets from genre pack inventory catalogs.

## Usage

```bash
cargo run -p sidequest-loadoutgen -- \
  --genre-packs-path ../sidequest-content/genre_packs \
  --genre low_fantasy \
  --class ranger \
  --tier 2
```

## What it generates

A loadout block with:
- Resolved items from the genre pack's `inventory.yaml`
- Starting gold and currency name
- Tier-scaled bonus items (tier 2+ adds higher-power items)
- Narrative hook ("You carry the standard kit of a...")
- Total equipment value

Output is JSON to stdout.
