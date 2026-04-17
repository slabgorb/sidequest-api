use sidequest_genre::models::archetype_axes::*;
use sidequest_genre::models::archetype_constraints::*;
use sidequest_genre::models::archetype_funnels::*;
use sidequest_genre::models::MechanicalEffects;

#[test]
fn test_deserialize_jungian_archetype() {
    let yaml = r#"
        id: sage
        drive: "Seeks truth and understanding"
        ocean_tendencies:
          openness: [7.0, 9.5]
          conscientiousness: [6.0, 8.0]
          extraversion: [2.0, 5.0]
          agreeableness: [4.0, 7.0]
          neuroticism: [3.0, 6.0]
        stat_affinity: [wisdom, intellect, insight]
    "#;
    let archetype: JungianArchetype = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(archetype.id, "sage");
    assert_eq!(archetype.stat_affinity.len(), 3);
    assert!((archetype.ocean_tendencies.openness[0] - 7.0).abs() < f64::EPSILON);
}

#[test]
fn test_deserialize_rpg_role() {
    let yaml = r#"
        id: healer
        combat_function: "Restores allies, removes conditions"
        stat_affinity: [wisdom, spirit, empathy]
    "#;
    let role: RpgRole = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(role.id, "healer");
    assert_eq!(role.combat_function, "Restores allies, removes conditions");
}

#[test]
fn test_deserialize_npc_role() {
    let yaml = r#"
        id: mook
        narrative_function: "Disposable opposition, exists to be overcome"
        skip_enrichment: true
    "#;
    let role: NpcRole = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(role.id, "mook");
    assert!(role.skip_enrichment);
}

#[test]
fn test_deserialize_npc_role_default_skip() {
    let yaml = r#"
        id: mentor
        narrative_function: "Guides protagonist"
    "#;
    let role: NpcRole = serde_yaml::from_str(yaml).unwrap();
    assert!(!role.skip_enrichment);
}

#[test]
fn test_deserialize_base_archetypes_file() {
    let yaml = r#"
        jungian:
          - id: sage
            drive: "Seeks truth"
            ocean_tendencies:
              openness: [7.0, 9.5]
              conscientiousness: [6.0, 8.0]
              extraversion: [2.0, 5.0]
              agreeableness: [4.0, 7.0]
              neuroticism: [3.0, 6.0]
            stat_affinity: [wisdom]
        rpg_roles:
          - id: healer
            combat_function: "Restores allies"
            stat_affinity: [wisdom]
        npc_roles:
          - id: mentor
            narrative_function: "Guides protagonist"
    "#;
    let base: BaseArchetypes = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(base.jungian.len(), 1);
    assert_eq!(base.rpg_roles.len(), 1);
    assert_eq!(base.npc_roles.len(), 1);
}

#[test]
fn test_deserialize_constraints() {
    let yaml = r#"
        valid_pairings:
          common:
            - [hero, tank]
            - [sage, healer]
          uncommon:
            - [jester, stealth]
          rare:
            - [innocent, tank]
          forbidden:
            - [innocent, stealth]
        genre_flavor:
          jungian:
            hero:
              speech_pattern: "direct"
              equipment_tendency: "weapons"
              visual_cues: "scarred hands"
          rpg_roles:
            tank:
              fallback_name: "Shield-Bearer"
        npc_roles_available:
          - mentor
          - mook
    "#;
    let constraints: ArchetypeConstraints = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(constraints.valid_pairings.common.len(), 2);
    assert_eq!(constraints.valid_pairings.forbidden.len(), 1);
    assert_eq!(
        constraints.genre_flavor.rpg_roles["tank"].fallback_name,
        "Shield-Bearer"
    );
}

#[test]
fn test_pairing_weight_lookup() {
    let yaml = r#"
        valid_pairings:
          common:
            - [hero, tank]
          uncommon:
            - [jester, stealth]
          rare:
            - [innocent, tank]
          forbidden:
            - [innocent, stealth]
        genre_flavor:
          jungian: {}
          rpg_roles: {}
        npc_roles_available: []
    "#;
    let constraints: ArchetypeConstraints = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        constraints.pairing_weight("hero", "tank"),
        Some(PairingWeight::Common)
    );
    assert_eq!(
        constraints.pairing_weight("jester", "stealth"),
        Some(PairingWeight::Uncommon)
    );
    assert_eq!(
        constraints.pairing_weight("innocent", "stealth"),
        Some(PairingWeight::Forbidden)
    );
    assert_eq!(constraints.pairing_weight("hero", "healer"), None);
}

#[test]
fn test_deserialize_funnel() {
    let yaml = r#"
        funnels:
          - name: Thornwall Mender
            absorbs:
              - [sage, healer]
              - [caregiver, healer]
            faction: Thornwall Convocation
            lore: "Itinerant healers"
            cultural_status: respected but watched
            disposition_toward:
              Iron Guard: cautious
          - name: Iron Warden
            absorbs:
              - [hero, tank]
            faction: Iron Guard
            lore: "Standing shield-wall"
            cultural_status: feared
        additional_constraints:
          forbidden: []
    "#;
    let funnels: ArchetypeFunnels = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(funnels.funnels.len(), 2);
    assert_eq!(funnels.funnels[0].absorbs.len(), 2);
    assert_eq!(
        funnels.funnels[0]
            .disposition_toward
            .get("Iron Guard")
            .unwrap(),
        "cautious"
    );
}

#[test]
fn test_funnel_resolution() {
    let yaml = r#"
        funnels:
          - name: Thornwall Mender
            absorbs:
              - [sage, healer]
              - [caregiver, healer]
            faction: Thornwall Convocation
            lore: "Itinerant healers"
            cultural_status: respected
          - name: Iron Warden
            absorbs:
              - [hero, tank]
            faction: Iron Guard
            lore: "Shield-wall"
            cultural_status: feared
        additional_constraints:
          forbidden: []
    "#;
    let funnels: ArchetypeFunnels = serde_yaml::from_str(yaml).unwrap();

    let result = funnels.resolve("sage", "healer");
    assert!(result.is_some());
    assert_eq!(result.unwrap().name, "Thornwall Mender");

    let result = funnels.resolve("hero", "tank");
    assert!(result.is_some());
    assert_eq!(result.unwrap().name, "Iron Warden");

    // Unmatched falls back
    assert!(funnels.resolve("jester", "dps").is_none());
}

#[test]
fn test_mechanical_effects_has_axis_fields() {
    let yaml = r#"
        jungian_hint: sage
        rpg_role_hint: healer
    "#;
    let effects: MechanicalEffects = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(effects.jungian_hint.as_deref(), Some("sage"));
    assert_eq!(effects.rpg_role_hint.as_deref(), Some("healer"));
}

#[test]
fn test_mechanical_effects_backward_compatible() {
    let yaml = r#"
        class_hint: Cleric
        race_hint: Human
        background: Farmer
    "#;
    let effects: MechanicalEffects = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(effects.class_hint.as_deref(), Some("Cleric"));
    assert!(effects.jungian_hint.is_none());
    assert!(effects.rpg_role_hint.is_none());
}
