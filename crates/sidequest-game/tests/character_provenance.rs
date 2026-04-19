//! Phase G3: Character carries the resolved archetype's provenance, and
//! a `ProvenancePanelExt` trait gives the GM-panel-facing tier label
//! used by dispatch's watcher events.

use sidequest_game::character::{Character, ProvenancePanelExt};
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_genre::archetype::ArchetypeResolved;
use sidequest_genre::resolver::Resolved;
use sidequest_protocol::{ContributionKind, MergeStep, NonBlankString, Provenance, Tier};
use std::collections::HashMap;
use std::path::PathBuf;

fn minimal_character() -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new("Rux").unwrap(),
            description: NonBlankString::new("A wandering mender").unwrap(),
            personality: NonBlankString::new("Patient").unwrap(),
            level: 1,
            edge: sidequest_game::creature_core::placeholder_edge_pool(),
            acquired_advancements: vec![],
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Raised by the Convocation").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Healer").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

fn culture_resolved() -> Resolved<ArchetypeResolved> {
    Resolved {
        value: ArchetypeResolved {
            name: "Thornwall Mender".into(),
            ..Default::default()
        },
        provenance: Provenance {
            source_tier: Tier::Culture,
            source_file: PathBuf::from("cultures/thornwall/archetype_reskins.yaml"),
            source_span: None,
            merge_trail: vec![MergeStep {
                tier: Tier::Culture,
                file: PathBuf::from("cultures/thornwall/archetype_reskins.yaml"),
                span: None,
                contribution: ContributionKind::Initial,
            }],
        },
    }
}

#[test]
fn apply_archetype_resolved_wires_name_and_provenance() {
    let mut c = minimal_character();
    c.apply_archetype_resolved(&culture_resolved());

    assert_eq!(c.resolved_archetype.as_deref(), Some("Thornwall Mender"));
    let prov = c
        .archetype_provenance
        .as_ref()
        .expect("provenance populated");
    assert_eq!(prov.source_tier, Tier::Culture);
    assert!(prov
        .source_file
        .to_string_lossy()
        .ends_with("archetype_reskins.yaml"));
}

#[test]
fn provenance_panel_ext_returns_culture_label() {
    let resolved = culture_resolved();
    assert_eq!(resolved.source_tier_for_panel(), "culture");
}

#[test]
fn provenance_panel_ext_covers_every_tier() {
    for (tier, want) in [
        (Tier::Global, "global"),
        (Tier::Genre, "genre"),
        (Tier::World, "world"),
        (Tier::Culture, "culture"),
    ] {
        let resolved = Resolved {
            value: ArchetypeResolved::default(),
            provenance: Provenance {
                source_tier: tier,
                source_file: PathBuf::from("whatever.yaml"),
                source_span: None,
                merge_trail: vec![],
            },
        };
        assert_eq!(resolved.source_tier_for_panel(), want);
    }
}
