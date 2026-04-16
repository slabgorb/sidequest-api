use sidequest_genre::archetype_resolve::*;
use sidequest_genre::models::archetype_axes::*;
use sidequest_genre::models::archetype_constraints::*;
use sidequest_genre::models::archetype_funnels::*;

fn test_base() -> BaseArchetypes {
    serde_yaml::from_str(
        r#"
        jungian:
          - id: sage
            drive: "Seeks truth"
            ocean_tendencies:
              openness: [7.0, 9.5]
              conscientiousness: [6.0, 8.0]
              extraversion: [2.0, 5.0]
              agreeableness: [4.0, 7.0]
              neuroticism: [3.0, 6.0]
            stat_affinity: [wisdom, intellect]
          - id: hero
            drive: "Proves worth"
            ocean_tendencies:
              openness: [5.0, 7.0]
              conscientiousness: [6.0, 8.5]
              extraversion: [6.0, 8.5]
              agreeableness: [5.0, 7.5]
              neuroticism: [2.0, 4.5]
            stat_affinity: [strength, endurance]
        rpg_roles:
          - id: healer
            combat_function: "Restores allies"
            stat_affinity: [wisdom]
          - id: tank
            combat_function: "Absorbs damage"
            stat_affinity: [strength]
        npc_roles:
          - id: mentor
            narrative_function: "Guides protagonist"
    "#,
    )
    .unwrap()
}

fn test_constraints() -> ArchetypeConstraints {
    serde_yaml::from_str(
        r#"
        valid_pairings:
          common:
            - [sage, healer]
            - [hero, tank]
          uncommon: []
          rare: []
          forbidden: []
        genre_flavor:
          jungian: {}
          rpg_roles:
            healer:
              fallback_name: "Hedge Healer"
            tank:
              fallback_name: "Shield-Bearer"
        npc_roles_available: [mentor]
    "#,
    )
    .unwrap()
}

fn test_funnels() -> ArchetypeFunnels {
    serde_yaml::from_str(
        r#"
        funnels:
          - name: Thornwall Mender
            absorbs:
              - [sage, healer]
            faction: Thornwall Convocation
            lore: "Itinerant healers"
            cultural_status: respected
        additional_constraints:
          forbidden: []
    "#,
    )
    .unwrap()
}

#[test]
fn test_resolve_with_world_funnel() {
    let base = test_base();
    let constraints = test_constraints();
    let funnels = Some(test_funnels());

    let result = resolve_archetype("sage", "healer", &base, &constraints, funnels.as_ref());
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.name, "Thornwall Mender");
    assert_eq!(resolved.faction.as_deref(), Some("Thornwall Convocation"));
    assert!(resolved.lore.contains("Itinerant"));
    assert_eq!(resolved.resolution_source, ResolutionSource::WorldFunnel);
}

#[test]
fn test_resolve_falls_back_to_genre() {
    let base = test_base();
    let constraints = test_constraints();

    let result = resolve_archetype("hero", "tank", &base, &constraints, None);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.name, "Shield-Bearer");
    assert!(resolved.faction.is_none());
    assert_eq!(resolved.resolution_source, ResolutionSource::GenreFallback);
}

#[test]
fn test_resolve_forbidden_pairing() {
    let base = test_base();
    let constraints: ArchetypeConstraints = serde_yaml::from_str(
        r#"
        valid_pairings:
          common: []
          uncommon: []
          rare: []
          forbidden:
            - [sage, tank]
        genre_flavor:
          jungian: {}
          rpg_roles: {}
        npc_roles_available: []
    "#,
    )
    .unwrap();

    let result = resolve_archetype("sage", "tank", &base, &constraints, None);
    assert!(result.is_err());
}

#[test]
fn test_resolve_unknown_axis_value() {
    let base = test_base();
    let constraints = test_constraints();

    let result = resolve_archetype("nonexistent", "healer", &base, &constraints, None);
    assert!(result.is_err());
}
