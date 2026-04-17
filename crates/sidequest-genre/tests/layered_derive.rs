use sidequest_genre::resolver::LayeredMerge;
use sidequest_genre::Layered;

#[derive(Debug, Clone, PartialEq, Layered)]
struct Archetype {
    #[layer(merge = "replace")]
    name: String,
    #[layer(merge = "append")]
    quirks: Vec<String>,
}

#[test]
fn layered_replace_field_uses_deeper_value() {
    let base = Archetype {
        name: "Base".into(),
        quirks: vec!["a".into()],
    };
    let deeper = Archetype {
        name: "Deeper".into(),
        quirks: vec!["b".into()],
    };
    let merged = base.merge(deeper);
    assert_eq!(merged.name, "Deeper");
}

#[test]
fn layered_append_field_concatenates() {
    let base = Archetype {
        name: "Base".into(),
        quirks: vec!["a".into()],
    };
    let deeper = Archetype {
        name: "Deeper".into(),
        quirks: vec!["b".into()],
    };
    let merged = base.merge(deeper);
    assert_eq!(merged.quirks, vec!["a", "b"]);
}
