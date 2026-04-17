use sidequest_genre::schema::genre::GenreContent;
use sidequest_genre::schema::global::GlobalContent;

#[test]
fn global_content_parses_minimal() {
    let yaml = r#"
jungian_axis: []
rpg_role_axis: []
npc_role_axis: []
"#;
    let parsed: GlobalContent = serde_yaml::from_str(yaml).unwrap();
    assert!(parsed.jungian_axis.is_empty());
    assert!(parsed.rpg_role_axis.is_empty());
    assert!(parsed.npc_role_axis.is_empty());
}

#[test]
fn global_content_rejects_unknown_field() {
    let yaml = r#"
jungian_axis: []
rpg_role_axis: []
npc_role_axis: []
funnels: []
"#;
    let result: Result<GlobalContent, _> = serde_yaml::from_str(yaml);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("funnels"), "expected error naming 'funnels', got: {err}");
}

#[test]
fn genre_content_parses_minimal() {
    let yaml = r#"
valid_pairings: {}
genre_flavor: {}
"#;
    let parsed: GenreContent = serde_yaml::from_str(yaml).unwrap();
    assert!(parsed.valid_pairings.is_empty());
}

#[test]
fn genre_content_rejects_funnels() {
    let yaml = r#"
valid_pairings: {}
genre_flavor: {}
funnels: []
"#;
    let result: Result<GenreContent, _> = serde_yaml::from_str(yaml);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("funnels"), "expected error naming 'funnels', got: {err}");
}
