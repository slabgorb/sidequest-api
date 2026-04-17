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

#[derive(Debug, Clone, PartialEq, Default, Layered)]
struct Nested {
    #[layer(merge = "replace")]
    inner: String,
}

#[derive(Debug, Clone, PartialEq, Default, Layered)]
struct Outer {
    #[layer(merge = "deep_merge")]
    nested: Nested,
    #[layer(merge = "culture_final")]
    culture_only: Option<String>,
}

#[test]
fn deep_merge_walks_into_nested_struct() {
    let base = Outer {
        nested: Nested {
            inner: "base".into(),
        },
        culture_only: None,
    };
    let deeper = Outer {
        nested: Nested {
            inner: "deeper".into(),
        },
        culture_only: Some("x".into()),
    };
    let merged = base.merge(deeper);
    assert_eq!(merged.nested.inner, "deeper");
    assert_eq!(merged.culture_only, Some("x".into()));
}

#[test]
fn culture_final_field_takes_deeper_value() {
    let base = Outer {
        nested: Nested::default(),
        culture_only: Some("from_base".into()),
    };
    let deeper = Outer {
        nested: Nested::default(),
        culture_only: Some("from_deeper".into()),
    };
    let merged = base.merge(deeper);
    assert_eq!(merged.culture_only, Some("from_deeper".into()));
}
