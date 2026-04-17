use sidequest_genre::resolver::merge::apply_append;
use sidequest_genre::resolver::merge::{apply_strategy, MergeStrategy};
use sidequest_genre::resolver::{ContributionKind, MergeStep, Provenance, Span, Tier};
use std::path::PathBuf;

#[test]
fn tier_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Tier::Global).unwrap(), "\"global\"");
    assert_eq!(serde_json::to_string(&Tier::Genre).unwrap(), "\"genre\"");
    assert_eq!(serde_json::to_string(&Tier::World).unwrap(), "\"world\"");
    assert_eq!(
        serde_json::to_string(&Tier::Culture).unwrap(),
        "\"culture\""
    );
}

#[test]
fn span_roundtrips() {
    let s = Span {
        start_line: 12,
        start_col: 1,
        end_line: 18,
        end_col: 0,
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: Span = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn provenance_round_trips_through_json() {
    let prov = Provenance {
        source_tier: Tier::World,
        source_file: PathBuf::from("worlds/evropi/archetype_funnels.yaml"),
        source_span: Some(Span {
            start_line: 12,
            start_col: 1,
            end_line: 18,
            end_col: 0,
        }),
        merge_trail: vec![
            MergeStep {
                tier: Tier::Genre,
                file: PathBuf::from("heavy_metal/archetype_constraints.yaml"),
                span: Some(Span {
                    start_line: 3,
                    start_col: 1,
                    end_line: 9,
                    end_col: 0,
                }),
                contribution: ContributionKind::Initial,
            },
            MergeStep {
                tier: Tier::World,
                file: PathBuf::from("worlds/evropi/archetype_funnels.yaml"),
                span: Some(Span {
                    start_line: 12,
                    start_col: 1,
                    end_line: 18,
                    end_col: 0,
                }),
                contribution: ContributionKind::Replaced,
            },
        ],
    };
    let json = serde_json::to_string(&prov).unwrap();
    let back: Provenance = serde_json::from_str(&json).unwrap();
    assert_eq!(prov, back);
}

#[test]
fn replace_strategy_returns_deeper() {
    let out = apply_strategy(MergeStrategy::Replace, Some("base"), Some("deeper"));
    assert_eq!(out, Some("deeper"));
}

#[test]
fn replace_strategy_keeps_base_when_deeper_absent() {
    let out = apply_strategy(MergeStrategy::Replace, Some("base"), None::<&str>);
    assert_eq!(out, Some("base"));
}

#[test]
fn append_strategy_concatenates_lists() {
    let base = vec!["a".to_string(), "b".to_string()];
    let deeper = vec!["c".to_string()];
    let out = apply_append(&base, &deeper);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn append_strategy_handles_empty_base() {
    let base: Vec<String> = vec![];
    let deeper = vec!["only".to_string()];
    let out = apply_append(&base, &deeper);
    assert_eq!(out, vec!["only"]);
}
