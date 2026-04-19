//! Story 39-5 — Authored advancement effects (genre-crate surface).
//!
//! These tests pin the type shape, serde contract, and loader behaviour
//! for the ADR-078 advancement system (GM-ratified amendments, 2026-04-15).
//!
//! They exercise the genre-crate half of the story:
//!
//!   * AC1 — `AdvancementEffect` enum has five v1 variants and round-trips
//!           through YAML/JSON serde; `LoreRevealScope` is the enum used by
//!           `LoreRevealBonus`.
//!   * AC1 — `#[non_exhaustive]` on `AdvancementEffect` so adding the four
//!           deferred ADR-079 variants does not break downstream matches.
//!   * AC1 — `RecoveryTrigger::OnBeatSuccess` gains `while_strained: bool`
//!           (exposed via `AdvancementEffect::EdgeRecovery { trigger, .. }`).
//!   * AC1 — `AdvancementTree` / `AdvancementTier` shape + serde.
//!   * AC2 — `AffinityTier.mechanical_effects: Option<Vec<AdvancementEffect>>`
//!           deserialises from inline YAML (heavy_metal's host location).
//!   * AC2 — live `heavy_metal/progression.yaml` has a tier carrying
//!           `mechanical_effects` (proves the content migration of draft §2).
//!   * AC2 — standalone `{genre}/advancements.yaml` file loads when the
//!           genre has no affinity-tier host.
//!   * AC2 — fails loudly when BOTH hosts are present for the same genre
//!           (project rule: no silent fallbacks).
//!
//! Red-signal imports — these unresolved paths are the compile failure Dev
//! must satisfy when implementing 39-5. Dev adds:
//!
//!   `sidequest-genre/src/models/advancement.rs` (new) —
//!     `pub struct AdvancementTree`
//!     `pub struct AdvancementTier`
//!     `pub enum AdvancementEffect`  (with `#[non_exhaustive]`)
//!     `pub enum LoreRevealScope`
//!
//!   `sidequest-genre/src/models/rules.rs` —
//!     extend `RecoveryTrigger::OnBeatSuccess` with `while_strained: bool`,
//!     OR move `RecoveryTrigger` into `sidequest-genre` if it is still in
//!     `sidequest-game` today (cross-crate cycle — see the deviation log
//!     in `sprint/39-5-session.md` under "TEA (test design)").
//!
//!   `sidequest-genre/src/models/progression.rs` —
//!     `AffinityTier.mechanical_effects: Option<Vec<AdvancementEffect>>`
//!
//!   `sidequest-genre/src/loader.rs` —
//!     detect-and-load `{genre}/advancements.yaml`; fail loudly when both
//!     `progression.yaml` tiers carry `mechanical_effects` AND a sibling
//!     `advancements.yaml` exists.

use std::path::PathBuf;

use sidequest_genre::{
    AdvancementEffect, AdvancementTier, AdvancementTree, AffinityTier, GenreError, LoreRevealScope,
    ProgressionConfig, RecoveryTrigger,
};

// ---------------------------------------------------------------------------
// Locator — find the live heavy_metal progression.yaml so AC2's content
// wiring test asserts against authored content, not a fixture.
// ---------------------------------------------------------------------------

fn heavy_metal_progression_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        for candidate in [
            "sidequest-content-39-5",
            "sidequest-content",
        ] {
            let path = ancestor
                .join(candidate)
                .join("genre_packs")
                .join("heavy_metal")
                .join("progression.yaml");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn fixture_genre_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// ---------------------------------------------------------------------------
// AC1 — AdvancementEffect variants + serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn advancement_effect_edge_max_bonus_serde_roundtrip() {
    let yaml = "type: edge_max_bonus\namount: 2\n";
    let effect: AdvancementEffect =
        serde_yaml::from_str(yaml).expect("edge_max_bonus must deserialize from canonical YAML");
    assert!(
        matches!(effect, AdvancementEffect::EdgeMaxBonus { amount: 2 }),
        "EdgeMaxBonus must preserve amount verbatim, got {:?}",
        effect
    );
    let back = serde_yaml::to_string(&effect).expect("serialize");
    let reparsed: AdvancementEffect = serde_yaml::from_str(&back).expect("round-trip reparse");
    assert!(
        matches!(reparsed, AdvancementEffect::EdgeMaxBonus { amount: 2 }),
        "round-trip must be lossless for EdgeMaxBonus"
    );
}

#[test]
fn advancement_effect_edge_recovery_carries_while_strained_flag() {
    // AC1 (GM amendment) — OnBeatSuccess gains `while_strained: bool`.
    // Strained = current <= max / 4 (matches UI "Cracked" state). The
    // trigger must carry the flag through serde so `progression.yaml` can
    // author "recovers 1 Edge on successful strike, but only while strained".
    let yaml = "type: edge_recovery\n\
                trigger:\n  kind: on_beat_success\n  beat_id: strike\n  amount: 1\n  while_strained: true\n\
                amount: 1\n";
    let effect: AdvancementEffect =
        serde_yaml::from_str(yaml).expect("edge_recovery with while_strained must deserialize");
    match effect {
        AdvancementEffect::EdgeRecovery { trigger, amount } => {
            assert_eq!(amount, 1, "EdgeRecovery.amount must survive serde");
            match trigger {
                RecoveryTrigger::OnBeatSuccess {
                    beat_id,
                    amount: trig_amount,
                    while_strained,
                } => {
                    assert_eq!(beat_id, "strike");
                    assert_eq!(trig_amount, 1);
                    assert!(
                        while_strained,
                        "while_strained=true must deserialise as true, not default-false"
                    );
                }
                other => panic!("expected OnBeatSuccess, got {:?}", other),
            }
        }
        other => panic!("expected EdgeRecovery variant, got {:?}", other),
    }
}

#[test]
fn advancement_effect_beat_discount_with_resource_mod_serde() {
    // AC1 (GM amendment) — BeatDiscount.resource_mod: Option<HashMap<String,i32>>
    // lets Pact-affinity tiers make pushes cheaper (voice:-1, ledger:-1).
    let yaml = "type: beat_discount\n\
                beat_id: pact_invocation\n\
                edge_delta_mod: -1\n\
                resource_mod:\n  voice: -1\n  ledger: -1\n";
    let effect: AdvancementEffect =
        serde_yaml::from_str(yaml).expect("beat_discount with resource_mod must deserialize");
    match effect {
        AdvancementEffect::BeatDiscount {
            beat_id,
            edge_delta_mod,
            resource_mod,
        } => {
            assert_eq!(beat_id, "pact_invocation");
            assert_eq!(edge_delta_mod, -1);
            let map = resource_mod.expect("resource_mod must be Some when YAML provides it");
            assert_eq!(
                map.get("voice").copied(),
                Some(-1),
                "voice mod must survive serde"
            );
            assert_eq!(
                map.get("ledger").copied(),
                Some(-1),
                "ledger mod must survive serde"
            );
        }
        other => panic!("expected BeatDiscount, got {:?}", other),
    }
}

#[test]
fn advancement_effect_beat_discount_without_resource_mod_is_none() {
    // Backward compatibility: tiers that only discount edge_delta (no push
    // currencies) must parse with resource_mod absent from the YAML.
    let yaml = "type: beat_discount\nbeat_id: strike\nedge_delta_mod: -1\n";
    let effect: AdvancementEffect = serde_yaml::from_str(yaml).expect("deserialize");
    match effect {
        AdvancementEffect::BeatDiscount {
            resource_mod: None, ..
        } => {}
        other => panic!(
            "expected BeatDiscount with resource_mod=None, got {:?}",
            other
        ),
    }
}

#[test]
fn advancement_effect_leverage_bonus_serde() {
    let yaml = "type: leverage_bonus\nbeat_id: strike\ntarget_edge_delta_mod: 1\n";
    let effect: AdvancementEffect = serde_yaml::from_str(yaml).expect("deserialize");
    assert!(
        matches!(
            effect,
            AdvancementEffect::LeverageBonus {
                beat_id: ref b,
                target_edge_delta_mod: 1,
            } if b == "strike"
        ),
        "LeverageBonus must preserve beat_id and mod, got {:?}",
        effect
    );
}

#[test]
fn advancement_effect_lore_reveal_bonus_serde() {
    // AC1 (GM amendment) — LoreRevealBonus { scope: LoreRevealScope } is in v1.
    let yaml = "type: lore_reveal_bonus\nscope: threshold_crossings\n";
    let effect: AdvancementEffect = serde_yaml::from_str(yaml).expect("deserialize");
    match effect {
        AdvancementEffect::LoreRevealBonus { scope } => {
            assert_eq!(
                scope,
                LoreRevealScope::ThresholdCrossings,
                "threshold_crossings YAML must map to ThresholdCrossings variant"
            );
        }
        other => panic!("expected LoreRevealBonus, got {:?}", other),
    }
}

#[test]
fn advancement_effect_is_non_exhaustive_for_adr_079_extensions() {
    // AC1 — AdvancementEffect must be #[non_exhaustive] so adding the four
    // deferred variants (AllyBeatDiscount, BetweenConfrontationsAction,
    // AllyEdgeGrant, EdgeThresholdDelay) in ADR-079 does NOT break
    // downstream `match` arms that only handle v1 variants.
    //
    // Behavioural proxy: any external match on the enum must include a
    // wildcard arm, or the test would fail to compile. We exercise this
    // by matching all five v1 variants explicitly AND a `_` arm; if the
    // attribute is missing, clippy/rustc will flag `_` as unreachable at
    // crate compile time for downstream crates. Here we simply prove the
    // wildcard is a legal arm (it compiles only with non_exhaustive OR
    // when an additional variant exists).
    //
    // Dev must add `#[non_exhaustive]` to the enum declaration.
    let effect = AdvancementEffect::EdgeMaxBonus { amount: 1 };
    let label = match &effect {
        AdvancementEffect::EdgeMaxBonus { .. } => "edge_max_bonus",
        AdvancementEffect::EdgeRecovery { .. } => "edge_recovery",
        AdvancementEffect::BeatDiscount { .. } => "beat_discount",
        AdvancementEffect::LeverageBonus { .. } => "leverage_bonus",
        AdvancementEffect::LoreRevealBonus { .. } => "lore_reveal_bonus",
        // Without `#[non_exhaustive]`, this arm is unreachable and clippy's
        // `unreachable_patterns` lint (denied at workspace level) fires.
        // The presence of the attribute is what makes this arm legal.
        _ => "unknown_future_variant",
    };
    assert_eq!(label, "edge_max_bonus");
}

// ---------------------------------------------------------------------------
// AC1 — AdvancementTree / AdvancementTier shape + serde
// ---------------------------------------------------------------------------

#[test]
fn advancement_tree_deserializes_with_tiers() {
    let yaml = concat!(
        "tiers:\n",
        "  - id: iron_1\n",
        "    required_milestone: iron_track\n",
        "    class_gates: [Fighter]\n",
        "    effects:\n",
        "      - type: edge_max_bonus\n",
        "        amount: 2\n",
        "  - id: pact_1\n",
        "    required_milestone: pact_track\n",
        "    class_gates: []\n",
        "    effects:\n",
        "      - type: beat_discount\n",
        "        beat_id: pact_invocation\n",
        "        edge_delta_mod: -1\n",
    );
    let tree: AdvancementTree =
        serde_yaml::from_str(yaml).expect("AdvancementTree must deserialize from canonical YAML");
    assert_eq!(tree.tiers.len(), 2, "tree must preserve tier order + count");

    let iron = &tree.tiers[0];
    assert_eq!(iron.id, "iron_1");
    assert_eq!(iron.required_milestone, "iron_track");
    assert_eq!(iron.class_gates, vec!["Fighter".to_string()]);
    assert_eq!(iron.effects.len(), 1);
    assert!(
        matches!(iron.effects[0], AdvancementEffect::EdgeMaxBonus { amount: 2 }),
        "iron_1 effect must be EdgeMaxBonus(2)"
    );

    let pact = &tree.tiers[1];
    assert!(
        pact.class_gates.is_empty(),
        "empty class_gates must mean universal (not a missing-field error)"
    );
}

#[test]
fn advancement_tier_id_must_not_be_blank() {
    // No silent fallbacks (CLAUDE.md) — a blank tier id would collide with
    // the "no tier granted" state in `acquired_advancements`. Deserialising
    // a blank id must be rejected loudly.
    let yaml = "tiers:\n\
                  - id: \"\"\n\
                    required_milestone: iron_track\n\
                    class_gates: []\n\
                    effects: []\n";
    let result: Result<AdvancementTree, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "AdvancementTier with blank id must fail deserialisation, got: {:?}",
        result.map(|t| t.tiers.into_iter().map(|x| x.id).collect::<Vec<_>>())
    );
}

// ---------------------------------------------------------------------------
// AC2 — AffinityTier.mechanical_effects field
// ---------------------------------------------------------------------------

#[test]
fn affinity_tier_mechanical_effects_deserializes_inline() {
    // The heavy_metal host location: affinity tier carries a
    // `mechanical_effects` list. Other packs with no such field keep
    // parsing (tested via absent-case below).
    let yaml = concat!(
        "name: \"Iron Affinity — Tier 1\"\n",
        "description: \"Durability awakens.\"\n",
        "abilities: []\n",
        "mechanical_effects:\n",
        "  - type: edge_max_bonus\n",
        "    amount: 2\n",
    );
    let tier: AffinityTier =
        serde_yaml::from_str(yaml).expect("AffinityTier must parse with mechanical_effects");
    let effects = tier
        .mechanical_effects
        .as_ref()
        .expect("mechanical_effects must be Some when YAML provides them");
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(effects[0], AdvancementEffect::EdgeMaxBonus { amount: 2 }),
        "effect must be EdgeMaxBonus(2)"
    );
}

#[test]
fn affinity_tier_without_mechanical_effects_parses_as_none() {
    // Backward compatibility: the other nine genres have no
    // mechanical_effects on their progression.yaml tiers and MUST still
    // load. mechanical_effects must be #[serde(default)] Option<Vec<_>>.
    let yaml = "name: \"Flavor Tier\"\n\
                description: \"Narrative only, no mechanical hook.\"\n\
                abilities: []\n";
    let tier: AffinityTier =
        serde_yaml::from_str(yaml).expect("AffinityTier without mechanical_effects must parse");
    assert!(
        tier.mechanical_effects.is_none(),
        "mechanical_effects absent from YAML must deserialise as None, got {:?}",
        tier.mechanical_effects
    );
}

#[test]
fn live_heavy_metal_progression_yaml_has_mechanical_effects() {
    // Content wiring test — proves draft §2 was lifted into the live
    // progression.yaml. If this fails, the content commit did not land
    // (or was lifted without the mechanical_effects block).
    let Some(path) = heavy_metal_progression_path() else {
        panic!(
            "heavy_metal progression.yaml not found — this test requires a sidequest-content \
             checkout next to sidequest-api, or a sidequest-content-39-5 worktree"
        );
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    let cfg: ProgressionConfig = serde_yaml::from_str(&text)
        .unwrap_or_else(|e| panic!("heavy_metal progression.yaml must parse: {}", e));

    let tiers_with_effects = cfg
        .affinities
        .iter()
        .filter_map(|a| a.unlocks.as_ref())
        .flat_map(|u| {
            [&u.tier_0, &u.tier_1, &u.tier_2, &u.tier_3]
                .into_iter()
                .filter_map(|t| t.as_ref())
        })
        .filter(|t| {
            t.mechanical_effects
                .as_ref()
                .is_some_and(|v| !v.is_empty())
        })
        .count();

    assert!(
        tiers_with_effects > 0,
        "live heavy_metal progression.yaml must carry mechanical_effects on at least one \
         affinity tier (draft §2 lift — Iron/Pact/Court/Ruin/Craft/Lore)"
    );
}

// ---------------------------------------------------------------------------
// AC2 — dual-location loader
// ---------------------------------------------------------------------------

#[test]
fn loader_reads_standalone_advancements_yaml_when_progression_has_no_effects() {
    // A genre without affinity-tier hosting uses the standalone
    // advancements.yaml sibling file. Dev must wire the loader to read it
    // into the genre pack's advancement tree.
    //
    // The fixture directory `tests/fixtures/standalone_advancements/`
    // contains `progression.yaml` (no mechanical_effects) and
    // `advancements.yaml` (the sole host).
    let dir = fixture_genre_dir("standalone_advancements");
    assert!(
        dir.is_dir(),
        "fixture must exist at {} — Dev or TEA creates it when implementing",
        dir.display()
    );

    let tree = sidequest_genre::load_advancement_tree(&dir)
        .expect("load_advancement_tree must succeed when only advancements.yaml is present");

    assert!(
        !tree.tiers.is_empty(),
        "standalone advancements.yaml fixture must yield at least one tier"
    );
}

#[test]
fn loader_fails_loudly_when_both_hosts_present() {
    // No silent fallbacks (CLAUDE.md + ADR-078 GM ruling). A genre must
    // pick ONE host. If both `progression.yaml` carries mechanical_effects
    // on any tier AND a sibling `advancements.yaml` exists, the loader
    // MUST return an error whose message names both files — picking one
    // silently would mask a genre-author bug.
    let dir = fixture_genre_dir("dual_host_conflict");
    assert!(
        dir.is_dir(),
        "fixture must exist at {} — Dev or TEA creates it when implementing",
        dir.display()
    );

    let err = sidequest_genre::load_advancement_tree(&dir)
        .expect_err("dual host MUST fail — no silent fallback");
    let msg = format!("{}", err);
    assert!(
        msg.contains("advancements.yaml") && msg.contains("progression.yaml"),
        "error message must name BOTH hosts so the genre author can resolve it; got: {}",
        msg
    );
    assert!(
        matches!(err, GenreError::ValidationError { .. } | GenreError::LoadError { .. }),
        "error variant should be a loader-level validation, not a generic IO error: {:?}",
        err
    );
}
