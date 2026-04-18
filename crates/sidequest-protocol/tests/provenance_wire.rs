//! Provenance wire-format roundtrip — ensures `Provenance` and its child
//! types ride on GameMessage payloads with a stable JSON shape.

use sidequest_protocol::{ContributionKind, MergeStep, Provenance, Span, Tier};
use std::path::PathBuf;

#[test]
fn provenance_roundtrips_through_protocol_json() {
    let p = Provenance {
        source_tier: Tier::World,
        source_file: PathBuf::from("worlds/evropi/world.yaml"),
        source_span: None,
        merge_trail: vec![],
    };
    let s = serde_json::to_string(&p).unwrap();
    let back: Provenance = serde_json::from_str(&s).unwrap();
    assert_eq!(p.source_tier, back.source_tier);
    assert_eq!(p.source_file, back.source_file);
    assert_eq!(p, back);
}

#[test]
fn tier_serializes_lowercase() {
    let s = serde_json::to_string(&Tier::Culture).unwrap();
    assert_eq!(s, "\"culture\"");
}

#[test]
fn contribution_kind_serializes_snake_case() {
    let s = serde_json::to_string(&ContributionKind::Initial).unwrap();
    assert_eq!(s, "\"initial\"");
}

#[test]
fn merge_step_with_span_roundtrips() {
    let step = MergeStep {
        tier: Tier::Culture,
        file: PathBuf::from("cultures/thornwall/archetype_reskins.yaml"),
        span: Some(Span {
            start_line: 12,
            start_col: 1,
            end_line: 18,
            end_col: 0,
        }),
        contribution: ContributionKind::Merged,
    };
    let s = serde_json::to_string(&step).unwrap();
    let back: MergeStep = serde_json::from_str(&s).unwrap();
    assert_eq!(step, back);
}
