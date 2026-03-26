# sidequest-genre

YAML genre pack loader, models, and validation.

Genre packs are swappable narrative configurations — each one defines a complete
game world with lore, rules, character creation, tropes, audio, and visual style.

## What's in here

- **`load_genre_pack(path)`** — Load and parse a genre pack directory
- **`GenreCache`** — Cache loaded packs to avoid re-parsing
- **`GenreCode`** — Validated genre identifier newtype
- **Models** — Strongly-typed structs for all 12 YAML files in a genre pack
- **Validation** — Schema and cross-reference validation
- **Trope inheritance** — `resolve_trope_inheritance()` merges parent/child trope definitions

## Usage

```rust
use std::path::Path;
let pack = sidequest_genre::load_genre_pack(Path::new("genre_packs/mutant_wasteland")).unwrap();
pack.validate().unwrap();
```

Genre pack format is documented in the original project's
[genre-packs.md](https://github.com/slabgorb/sq-2/blob/main/docs/genre-packs.md).
See also [ADR-003](../../../docs/adr/003-genre-pack-architecture.md).
