//! Story 10-6: World state agent proposes OCEAN shifts — game events trigger
//! personality evolution.
//!
//! RED phase: tests compile against stub types but FAIL because the mapping
//! logic is not yet implemented.
//!
//! Acceptance criteria:
//!   AC-1: OceanShiftProposal struct with npc_name, dimension, delta, cause
//!   AC-2: Event-to-shift mapping — 5+ event types produce appropriate proposals
//!   AC-3: propose_ocean_shifts() returns Vec<OceanShiftProposal>
//!   AC-4: Delta validation — abs(delta) <= 2.0
//!   AC-5: Proposals are applicable via apply_shift() on OceanProfile

use sidequest_game::{
    propose_ocean_shifts, OceanDimension, OceanProfile, OceanShiftLog, OceanShiftProposal,
    PersonalityEvent,
};

// ─── AC-1: OceanShiftProposal struct fields ─────────────────

#[test]
fn proposal_has_required_fields() {
    let proposal = OceanShiftProposal {
        npc_name: "Griselda".to_string(),
        dimension: OceanDimension::Agreeableness,
        delta: -1.5,
        cause: "betrayed by a trusted ally".to_string(),
    };
    assert_eq!(proposal.npc_name, "Griselda");
    assert_eq!(proposal.dimension, OceanDimension::Agreeableness);
    assert!((proposal.delta - (-1.5)).abs() < f64::EPSILON);
    assert_eq!(proposal.cause, "betrayed by a trusted ally");
}

#[test]
fn proposal_supports_clone_and_debug() {
    let proposal = OceanShiftProposal {
        npc_name: "Kael".to_string(),
        dimension: OceanDimension::Neuroticism,
        delta: 1.0,
        cause: "near-death experience".to_string(),
    };
    let cloned = proposal.clone();
    assert_eq!(format!("{:?}", cloned), format!("{:?}", proposal));
}

// ─── AC-2: Event-to-shift mapping ──────────────────────────

#[test]
fn personality_event_enum_has_all_variants() {
    // Ensure all five narrative event variants exist.
    let _events = [
        PersonalityEvent::Betrayal,
        PersonalityEvent::NearDeath,
        PersonalityEvent::Victory,
        PersonalityEvent::Defeat,
        PersonalityEvent::SocialBonding,
    ];
}

#[test]
fn betrayal_lowers_agreeableness_raises_neuroticism() {
    let proposals = propose_ocean_shifts(PersonalityEvent::Betrayal, "Mira");
    assert!(
        !proposals.is_empty(),
        "betrayal should produce at least one proposal"
    );

    let agreeableness_shift = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Agreeableness);
    assert!(
        agreeableness_shift.is_some(),
        "betrayal should propose an Agreeableness shift"
    );
    assert!(
        agreeableness_shift.unwrap().delta < 0.0,
        "betrayal should lower Agreeableness"
    );

    let neuroticism_shift = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Neuroticism);
    assert!(
        neuroticism_shift.is_some(),
        "betrayal should propose a Neuroticism shift"
    );
    assert!(
        neuroticism_shift.unwrap().delta > 0.0,
        "betrayal should raise Neuroticism"
    );
}

#[test]
fn near_death_raises_neuroticism() {
    let proposals = propose_ocean_shifts(PersonalityEvent::NearDeath, "Torval");
    assert!(!proposals.is_empty(), "near-death should produce proposals");

    let neuro = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Neuroticism)
        .expect("near-death should propose a Neuroticism shift");
    assert!(neuro.delta > 0.0, "near-death should raise Neuroticism");
}

#[test]
fn victory_raises_conscientiousness_and_extraversion() {
    let proposals = propose_ocean_shifts(PersonalityEvent::Victory, "Anya");

    let consc = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Conscientiousness)
        .expect("victory should propose a Conscientiousness shift");
    assert!(consc.delta > 0.0, "victory should raise Conscientiousness");

    let extra = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Extraversion)
        .expect("victory should propose an Extraversion shift");
    assert!(extra.delta > 0.0, "victory should raise Extraversion");
}

#[test]
fn defeat_raises_neuroticism_lowers_extraversion() {
    let proposals = propose_ocean_shifts(PersonalityEvent::Defeat, "Sable");

    let neuro = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Neuroticism)
        .expect("defeat should propose a Neuroticism shift");
    assert!(neuro.delta > 0.0, "defeat should raise Neuroticism");

    let extra = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Extraversion)
        .expect("defeat should propose an Extraversion shift");
    assert!(extra.delta < 0.0, "defeat should lower Extraversion");
}

#[test]
fn social_bonding_raises_agreeableness_and_extraversion() {
    let proposals = propose_ocean_shifts(PersonalityEvent::SocialBonding, "Lark");

    let agree = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Agreeableness)
        .expect("social bonding should propose an Agreeableness shift");
    assert!(
        agree.delta > 0.0,
        "social bonding should raise Agreeableness"
    );

    let extra = proposals
        .iter()
        .find(|p| p.dimension == OceanDimension::Extraversion)
        .expect("social bonding should propose an Extraversion shift");
    assert!(
        extra.delta > 0.0,
        "social bonding should raise Extraversion"
    );
}

#[test]
fn proposals_carry_npc_name() {
    let proposals = propose_ocean_shifts(PersonalityEvent::Victory, "Fennec");
    for p in &proposals {
        assert_eq!(
            p.npc_name, "Fennec",
            "all proposals should carry the NPC name"
        );
    }
}

#[test]
fn proposals_carry_cause_string() {
    let proposals = propose_ocean_shifts(PersonalityEvent::Betrayal, "Wren");
    for p in &proposals {
        assert!(
            !p.cause.is_empty(),
            "each proposal should have a non-empty cause"
        );
    }
}

// ─── AC-3: propose_ocean_shifts() returns Vec ──────────────

#[test]
fn propose_returns_vec_for_each_event_type() {
    let events = [
        PersonalityEvent::Betrayal,
        PersonalityEvent::NearDeath,
        PersonalityEvent::Victory,
        PersonalityEvent::Defeat,
        PersonalityEvent::SocialBonding,
    ];
    for event in events {
        let proposals = propose_ocean_shifts(event, "TestNpc");
        assert!(
            !proposals.is_empty(),
            "event {:?} should produce at least one proposal",
            event
        );
    }
}

// ─── AC-4: Delta validation — abs(delta) <= 2.0 ────────────

#[test]
fn all_proposal_deltas_are_capped() {
    let events = [
        PersonalityEvent::Betrayal,
        PersonalityEvent::NearDeath,
        PersonalityEvent::Victory,
        PersonalityEvent::Defeat,
        PersonalityEvent::SocialBonding,
    ];
    for event in events {
        let proposals = propose_ocean_shifts(event, "DeltaCheck");
        for p in &proposals {
            assert!(
                p.delta.abs() <= 2.0,
                "proposal delta {} for {:?}/{:?} exceeds cap of 2.0",
                p.delta,
                event,
                p.dimension
            );
        }
    }
}

#[test]
fn no_zero_deltas() {
    let events = [
        PersonalityEvent::Betrayal,
        PersonalityEvent::NearDeath,
        PersonalityEvent::Victory,
        PersonalityEvent::Defeat,
        PersonalityEvent::SocialBonding,
    ];
    for event in events {
        let proposals = propose_ocean_shifts(event, "NonZero");
        for p in &proposals {
            assert!(
                p.delta.abs() > f64::EPSILON,
                "proposal delta should not be zero for {:?}/{:?}",
                event,
                p.dimension
            );
        }
    }
}

// ─── AC-5: Proposals are applicable via apply_shift() ──────

#[test]
fn proposals_can_be_applied_to_ocean_profile() {
    let mut profile = OceanProfile::default(); // all 5.0
    let mut log = OceanShiftLog::default();

    let proposals = propose_ocean_shifts(PersonalityEvent::Betrayal, "Viktor");

    for p in &proposals {
        profile.apply_shift(p.dimension, p.delta, p.cause.clone(), 1, &mut log);
    }

    // At least one dimension should have changed from the default 5.0
    let changed = [
        OceanDimension::Openness,
        OceanDimension::Conscientiousness,
        OceanDimension::Extraversion,
        OceanDimension::Agreeableness,
        OceanDimension::Neuroticism,
    ]
    .iter()
    .any(|&dim| (profile.get(dim) - 5.0).abs() > f64::EPSILON);

    assert!(
        changed,
        "applying proposals should change at least one dimension"
    );
    assert!(
        !log.shifts().is_empty(),
        "applying proposals should add entries to the shift log"
    );
}

#[test]
fn applied_proposals_stay_within_bounds() {
    // Start at extremes to test clamping with proposals
    let mut profile = OceanProfile {
        openness: 9.5,
        conscientiousness: 0.5,
        extraversion: 9.5,
        agreeableness: 0.5,
        neuroticism: 9.5,
    };
    let mut log = OceanShiftLog::default();

    // Apply all event types' proposals to stress-test bounds
    let events = [
        PersonalityEvent::Betrayal,
        PersonalityEvent::NearDeath,
        PersonalityEvent::Victory,
        PersonalityEvent::Defeat,
        PersonalityEvent::SocialBonding,
    ];
    for event in events {
        let proposals = propose_ocean_shifts(event, "Boundary");
        for p in &proposals {
            profile.apply_shift(p.dimension, p.delta, p.cause.clone(), 1, &mut log);
        }
    }

    // All dimensions must stay in [0.0, 10.0]
    for dim in [
        OceanDimension::Openness,
        OceanDimension::Conscientiousness,
        OceanDimension::Extraversion,
        OceanDimension::Agreeableness,
        OceanDimension::Neuroticism,
    ] {
        let val = profile.get(dim);
        assert!(
            (0.0..=10.0).contains(&val),
            "{:?} = {} is out of bounds after applying proposals",
            dim,
            val
        );
    }
}
