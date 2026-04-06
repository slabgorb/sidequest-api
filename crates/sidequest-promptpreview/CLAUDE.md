# sidequest-promptpreview — Feature Inventory

CLI binary that renders the fully composed narrator prompt using real Rust types.
**~330 LOC, single file (`main.rs`), fully complete.**

Replaces `scripts/preview-prompt.py` which duplicated prompt constants and drifted
from the Rust codebase. This binary uses the actual `NarratorAgent`, `ContextBuilder`,
`PromptRegistry`, and SOUL parser — the prompt can never drift because it IS the
same code path the server uses.

## COMPLETE — Do Not Rewrite

- **Labeled output** — zone annotations, section names, categories, token estimates
- **Raw output** (`--raw`) — plain text as Claude sees it
- **Test mode** (`--test`) — pipes raw prompt to `claude -p` for immediate evaluation
- **Seed mode** (`--seed`) — shells out to `sidequest-namegen` / `sidequest-encountergen`
  for real NPC/encounter data instead of static placeholders
- **Conditional modes** — `--combat`, `--chase`, `--dialogue` inject ADR-067 rules
- **Verbosity/vocabulary** — `--verbosity` and `--vocabulary` use the real
  `PromptRegistry` injection (same text the server uses)
- **Custom actions** — `--action "text"` overrides the player action placeholder

## CLI Arguments

| Flag | Required | Description |
|------|----------|-------------|
| `--raw` | No | Strip zone labels, show plain text |
| `--test` | No | Pipe to `claude -p` |
| `--seed` | No | Real NPCs/encounters from binaries |
| `--genre` | No | Genre for `--seed` (default: mutant_wasteland) |
| `--genre-packs-path` | No | Auto-detected if omitted |
| `--combat` | No | Include combat narration rules |
| `--chase` | No | Include chase narration rules |
| `--dialogue` | No | Include dialogue narration rules |
| `--verbosity` | No | concise/standard/verbose (default: standard) |
| `--vocabulary` | No | accessible/literary/epic (default: literary) |
| `--action` | No | Custom player action text |
| `--model` | No | Claude model for `--test` (default: sonnet) |

## Justfile

```bash
just prompt-preview                          # labeled output
just prompt-preview --raw                    # raw output
just prompt-preview --test --combat          # test combat prompt
just prompt-preview --seed --genre neon_dystopia  # seeded NPCs
```
