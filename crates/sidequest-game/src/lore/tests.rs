use super::*;
use crate::conlang::Morpheme;
use std::collections::HashMap;

fn sample_metadata() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("author".to_string(), "narrator".to_string());
    m.insert("region".to_string(), "flickering_reach".to_string());
    m
}

fn sample_fragment() -> LoreFragment {
    LoreFragment::new(
        "lore-001".to_string(),
        LoreCategory::History,
        "The Flickering Reach was once a thriving trade hub.".to_string(),
        LoreSource::GenrePack,
        Some(5),
        sample_metadata(),
    )
}

// === Constructor and field storage ===

#[test]
fn new_stores_id() {
    let frag = sample_fragment();
    assert_eq!(frag.id(), "lore-001");
}

#[test]
fn new_stores_category() {
    let frag = sample_fragment();
    assert_eq!(frag.category(), &LoreCategory::History);
}

#[test]
fn new_stores_content() {
    let frag = sample_fragment();
    assert_eq!(
        frag.content(),
        "The Flickering Reach was once a thriving trade hub."
    );
}

#[test]
fn new_stores_source() {
    let frag = sample_fragment();
    assert_eq!(frag.source(), &LoreSource::GenrePack);
}

#[test]
fn new_stores_turn_created() {
    let frag = sample_fragment();
    assert_eq!(frag.turn_created(), Some(5));
}

#[test]
fn new_stores_metadata() {
    let frag = sample_fragment();
    assert_eq!(frag.metadata().get("author").unwrap(), "narrator");
    assert_eq!(frag.metadata().get("region").unwrap(), "flickering_reach");
}

#[test]
fn new_with_none_turn_created() {
    let frag = LoreFragment::new(
        "lore-002".to_string(),
        LoreCategory::Geography,
        "Mountains to the north.".to_string(),
        LoreSource::GameEvent,
        None,
        HashMap::new(),
    );
    assert_eq!(frag.turn_created(), None);
}

// === Token estimation ===

#[test]
fn token_estimate_100_chars() {
    // 100 chars ÷ 4 = 25 tokens
    let content = "a".repeat(100);
    let frag = LoreFragment::new(
        "tok-100".to_string(),
        LoreCategory::Event,
        content,
        LoreSource::GameEvent,
        None,
        HashMap::new(),
    );
    assert_eq!(frag.token_estimate(), 25);
}

#[test]
fn token_estimate_short_string() {
    // 7 chars → ceil(7/4) = 2 tokens
    let frag = LoreFragment::new(
        "tok-short".to_string(),
        LoreCategory::Item,
        "hello!!".to_string(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    assert_eq!(frag.token_estimate(), 2);
}

#[test]
fn token_estimate_empty_string() {
    let frag = LoreFragment::new(
        "tok-empty".to_string(),
        LoreCategory::Language,
        String::new(),
        LoreSource::CharacterCreation,
        None,
        HashMap::new(),
    );
    assert_eq!(frag.token_estimate(), 0);
}

#[test]
fn token_estimate_one_char() {
    let frag = LoreFragment::new(
        "tok-1".to_string(),
        LoreCategory::Character,
        "x".to_string(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    assert_eq!(frag.token_estimate(), 1);
}

#[test]
fn constructor_auto_computes_token_estimate() {
    let frag = sample_fragment();
    // "The Flickering Reach was once a thriving trade hub." = 52 chars
    // 52 / 4 = 13 tokens
    assert_eq!(frag.token_estimate(), 13);
}

// === LoreCategory variants ===

#[test]
fn all_fixed_categories_are_distinct() {
    let categories = vec![
        LoreCategory::History,
        LoreCategory::Geography,
        LoreCategory::Faction,
        LoreCategory::Character,
        LoreCategory::Item,
        LoreCategory::Event,
        LoreCategory::Language,
    ];
    for (i, a) in categories.iter().enumerate() {
        for (j, b) in categories.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn custom_category_holds_value() {
    let cat = LoreCategory::Custom("Prophecy".to_string());
    if let LoreCategory::Custom(ref s) = cat {
        assert_eq!(s, "Prophecy");
    } else {
        panic!("Expected Custom variant");
    }
}

#[test]
fn custom_categories_with_different_values_are_distinct() {
    let a = LoreCategory::Custom("Prophecy".to_string());
    let b = LoreCategory::Custom("Religion".to_string());
    assert_ne!(a, b);
}

// === LoreSource variants ===

#[test]
fn all_sources_are_distinct() {
    let sources = vec![
        LoreSource::GenrePack,
        LoreSource::CharacterCreation,
        LoreSource::GameEvent,
    ];
    for (i, a) in sources.iter().enumerate() {
        for (j, b) in sources.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// === Serde round-trip ===

#[test]
fn serde_json_round_trip() {
    let frag = sample_fragment();
    let json = serde_json::to_string(&frag).expect("serialize");
    let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.id(), frag.id());
    assert_eq!(restored.category(), frag.category());
    assert_eq!(restored.content(), frag.content());
    assert_eq!(restored.token_estimate(), frag.token_estimate());
    assert_eq!(restored.source(), frag.source());
    assert_eq!(restored.turn_created(), frag.turn_created());
    assert_eq!(restored.metadata(), frag.metadata());
}

#[test]
fn serde_round_trip_custom_category() {
    let frag = LoreFragment::new(
        "custom-001".to_string(),
        LoreCategory::Custom("Prophecy".to_string()),
        "The chosen one will rise.".to_string(),
        LoreSource::CharacterCreation,
        Some(10),
        HashMap::new(),
    );
    let json = serde_json::to_string(&frag).expect("serialize");
    let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.category(), &LoreCategory::Custom("Prophecy".to_string()));
}

#[test]
fn serde_round_trip_with_metadata() {
    let frag = sample_fragment();
    let json = serde_json::to_string(&frag).expect("serialize");
    let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.metadata().len(), 2);
    assert_eq!(restored.metadata().get("author").unwrap(), "narrator");
}

// === Metadata ===

#[test]
fn empty_metadata_is_valid() {
    let frag = LoreFragment::new(
        "meta-empty".to_string(),
        LoreCategory::Faction,
        "Some faction lore.".to_string(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    assert!(frag.metadata().is_empty());
}

#[test]
fn metadata_supports_arbitrary_keys() {
    let mut meta = HashMap::new();
    meta.insert("custom_key".to_string(), "custom_value".to_string());
    meta.insert("another".to_string(), "entry".to_string());
    meta.insert("number".to_string(), "42".to_string());

    let frag = LoreFragment::new(
        "meta-arb".to_string(),
        LoreCategory::Item,
        "A mysterious artifact.".to_string(),
        LoreSource::GameEvent,
        Some(3),
        meta,
    );
    assert_eq!(frag.metadata().len(), 3);
    assert_eq!(frag.metadata().get("number").unwrap(), "42");
}

// ===================================================================
// LoreStore tests (story 11-2)
// ===================================================================

fn history_fragment() -> LoreFragment {
    LoreFragment::new(
        "lore-hist-001".to_string(),
        LoreCategory::History,
        "The Flickering Reach was once a thriving trade hub.".to_string(),
        LoreSource::GenrePack,
        Some(1),
        HashMap::new(),
    )
}

fn geography_fragment() -> LoreFragment {
    LoreFragment::new(
        "lore-geo-001".to_string(),
        LoreCategory::Geography,
        "The northern mountains are impassable in winter.".to_string(),
        LoreSource::GenrePack,
        Some(2),
        HashMap::new(),
    )
}

fn faction_fragment() -> LoreFragment {
    LoreFragment::new(
        "lore-fac-001".to_string(),
        LoreCategory::Faction,
        "The Merchant Guild controls all trade routes through the Reach.".to_string(),
        LoreSource::GameEvent,
        Some(3),
        HashMap::new(),
    )
}

// --- Construction ---

#[test]
fn lore_store_new_is_empty() {
    let store = LoreStore::new();
    assert!(store.is_empty());
}

#[test]
fn lore_store_new_has_zero_len() {
    let store = LoreStore::new();
    assert_eq!(store.len(), 0);
}

#[test]
fn lore_store_new_has_zero_tokens() {
    let store = LoreStore::new();
    assert_eq!(store.total_tokens(), 0);
}

// --- Add fragment ---

#[test]
fn lore_store_add_increases_len() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    assert_eq!(store.len(), 1);
}

#[test]
fn lore_store_add_makes_non_empty() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    assert!(!store.is_empty());
}

#[test]
fn lore_store_add_multiple() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    store.add(faction_fragment()).unwrap();
    assert_eq!(store.len(), 3);
}

// --- Query by category ---

#[test]
fn lore_store_query_by_category_returns_matches() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    store.add(faction_fragment()).unwrap();

    let results = store.query_by_category(&LoreCategory::History);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id(), "lore-hist-001");
}

#[test]
fn lore_store_query_by_category_no_matches() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();

    let results = store.query_by_category(&LoreCategory::Item);
    assert!(results.is_empty());
}

#[test]
fn lore_store_query_by_category_multiple_matches() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(LoreFragment::new(
        "lore-hist-002".to_string(),
        LoreCategory::History,
        "The great war ended five centuries ago.".to_string(),
        LoreSource::GenrePack,
        Some(4),
        HashMap::new(),
    )).unwrap();

    let results = store.query_by_category(&LoreCategory::History);
    assert_eq!(results.len(), 2);
}

// --- Query by keyword ---

#[test]
fn lore_store_query_by_keyword_returns_matches() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    store.add(faction_fragment()).unwrap();

    // "merchant" appears in faction fragment content
    let results = store.query_by_keyword("merchant");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id(), "lore-fac-001");
}

#[test]
fn lore_store_query_by_keyword_case_insensitive() {
    let mut store = LoreStore::new();
    store.add(faction_fragment()).unwrap();

    let results = store.query_by_keyword("MERCHANT");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id(), "lore-fac-001");
}

#[test]
fn lore_store_query_by_keyword_no_matches() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();

    let results = store.query_by_keyword("dragon");
    assert!(results.is_empty());
}

#[test]
fn lore_store_query_by_keyword_multiple_matches() {
    let mut store = LoreStore::new();
    // Both contain "the"
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    store.add(faction_fragment()).unwrap();

    let results = store.query_by_keyword("the");
    assert_eq!(results.len(), 3);
}

// --- Token budget ---

#[test]
fn lore_store_total_tokens_single() {
    let mut store = LoreStore::new();
    let frag = history_fragment();
    let expected = frag.token_estimate();
    store.add(frag).unwrap();
    assert_eq!(store.total_tokens(), expected);
}

#[test]
fn lore_store_total_tokens_multiple() {
    let mut store = LoreStore::new();
    let h = history_fragment();
    let g = geography_fragment();
    let f = faction_fragment();
    let expected = h.token_estimate() + g.token_estimate() + f.token_estimate();
    store.add(h).unwrap();
    store.add(g).unwrap();
    store.add(f).unwrap();
    assert_eq!(store.total_tokens(), expected);
}

// --- Duplicate detection ---

#[test]
fn lore_store_duplicate_id_rejected() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();

    // Same id, different content
    let dup = LoreFragment::new(
        "lore-hist-001".to_string(),
        LoreCategory::History,
        "Completely different content.".to_string(),
        LoreSource::GameEvent,
        None,
        HashMap::new(),
    );
    assert!(store.add(dup).is_err());
}

#[test]
fn lore_store_duplicate_rejected_preserves_original() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();

    let dup = LoreFragment::new(
        "lore-hist-001".to_string(),
        LoreCategory::History,
        "Completely different content.".to_string(),
        LoreSource::GameEvent,
        None,
        HashMap::new(),
    );
    let _ = store.add(dup);

    // Original is still there
    let results = store.query_by_category(&LoreCategory::History);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].content(),
        "The Flickering Reach was once a thriving trade hub."
    );
}

#[test]
fn lore_store_duplicate_rejected_len_unchanged() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    let _ = store.add(LoreFragment::new(
        "lore-hist-001".to_string(),
        LoreCategory::Event,
        "Duplicate id.".to_string(),
        LoreSource::GameEvent,
        None,
        HashMap::new(),
    ));
    assert_eq!(store.len(), 1);
}

// ===================================================================
// Lore seeding tests (story 11-3)
// ===================================================================

use sidequest_genre::{
    CharCreationChoice, CharCreationScene, Faction, GenrePack, Lore, MechanicalEffects,
};
use super::{seed_lore_from_char_creation, seed_lore_from_genre_pack};

/// Build a minimal GenrePack with only the fields relevant to lore seeding.
/// Non-lore fields are filled with harmless defaults via serde deserialization.
fn test_genre_pack(lore: Lore, char_creation: Vec<CharCreationScene>) -> GenrePack {
    use sidequest_genre::*;
    use sidequest_protocol::NonBlankString;

    GenrePack {
        meta: PackMeta {
            name: NonBlankString::new("test-pack").unwrap(),
            version: "0.1.0".to_string(),
            description: "Test genre pack".to_string(),
            min_sidequest_version: "0.1.0".to_string(),
            refine_hooks: None,
            inspirations: vec![],
            era_range: None,
            core_vibe: None,
            emotional_tone: vec![],
            differentiation: None,
        },
        rules: serde_json::from_value(serde_json::json!({
            "magic_level": "none",
            "stat_generation": "point_buy",
            "point_buy_budget": 27,
            "ability_score_names": ["STR", "DEX", "CON", "INT", "WIS", "CHA"],
            "allowed_classes": ["fighter"],
            "allowed_races": ["human"],
            "class_hp_bases": {"fighter": 10}
        })).unwrap(),
        lore,
        theme: serde_json::from_value(serde_json::json!({
            "primary": "#000",
            "secondary": "#fff",
            "accent": "#f00",
            "background": "#111",
            "surface": "#222",
            "text": "#eee",
            "border_style": "solid",
            "web_font_family": "monospace",
            "dinkus": {
                "enabled": false,
                "cooldown": 3,
                "default_weight": "light",
                "glyph": {}
            },
            "session_opener": { "enabled": false }
        })).unwrap(),
        archetypes: vec![],
        char_creation,
        visual_style: serde_json::from_value(serde_json::json!({
            "positive_suffix": "test",
            "negative_prompt": "",
            "preferred_model": "test",
            "base_seed": 42
        })).unwrap(),
        progression: serde_json::from_value(serde_json::json!({})).unwrap(),
        axes: serde_json::from_value(serde_json::json!({
            "definitions": []
        })).unwrap(),
        audio: serde_json::from_value(serde_json::json!({
            "mood_tracks": {},
            "sfx_library": {},
            "creature_voice_presets": {},
            "mixer": {
                "music_volume": 0.5,
                "sfx_volume": 0.5,
                "voice_volume": 0.8,
                "duck_music_for_voice": true,
                "duck_amount_db": -6.0,
                "crossfade_default_ms": 2000
            }
        })).unwrap(),
        cultures: vec![],
        prompts: serde_json::from_value(serde_json::json!({
            "narrator": "test",
            "combat": "test",
            "npc": "test",
            "world_state": "test"
        })).unwrap(),
        tropes: vec![],
        beat_vocabulary: None,
        achievements: vec![],
        voice_presets: None,
        power_tiers: HashMap::new(),
        worlds: HashMap::new(),
        scenarios: HashMap::new(),
        drama_thresholds: None,
        openings: vec![],
        inventory: None,
        backstory_tables: None,
    }
}

fn test_lore() -> Lore {
    Lore {
        world_name: "The Shattered Reach".to_string(),
        history: "Three generations ago, the Reach was one kingdom.".to_string(),
        geography: "The Shattered Reach spans a broad river valley.".to_string(),
        cosmology: "The people hold no unified theology.".to_string(),
        factions: vec![
            Faction {
                name: "The Merchant Consortium".to_string(),
                summary: "Trade coalition enforcing the Iron Accord".to_string(),
                description: "A coalition of wealthy trading families.".to_string(),
                disposition: "neutral".to_string(),
                extras: HashMap::new(),
            },
            Faction {
                name: "Order of the Ashen Veil".to_string(),
                summary: "Anti-magic inquisitors devoted to the old gods".to_string(),
                description: "A religious order devoted to the old gods.".to_string(),
                disposition: "hostile".to_string(),
                extras: HashMap::new(),
            },
        ],
        extras: HashMap::new(),
    }
}

fn test_mechanical_effects() -> MechanicalEffects {
    MechanicalEffects {
        background: Some("farmhand".to_string()),
        ..MechanicalEffects::default()
    }
}

fn test_char_creation_scenes() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "origins".to_string(),
            title: "Your Origins".to_string(),
            narration: "Where did you come from?".to_string(),
            choices: vec![
                CharCreationChoice {
                    label: "The Hearthlands".to_string(),
                    description: "You grew up on a quiet farm.".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
                CharCreationChoice {
                    label: "The Stone Halls".to_string(),
                    description: "You were raised underground.".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
            ],
            allows_freeform: None,
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "motivation".to_string(),
            title: "Your Drive".to_string(),
            narration: "What drives you?".to_string(),
            choices: vec![CharCreationChoice {
                label: "Revenge".to_string(),
                description: "Someone wronged you.".to_string(),
                mechanical_effects: test_mechanical_effects(),
            }],
            allows_freeform: None,
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
    ]
}

// --- Genre pack lore seeding ---

#[test]
fn seed_genre_pack_history_fragment() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let results = store.query_by_category(&LoreCategory::History);
    let history = results.iter().find(|f| f.id() == "lore_genre_history");
    assert!(history.is_some(), "expected lore_genre_history fragment");
    let h = history.unwrap();
    assert!(h.content().contains("Three generations ago"));
    assert_eq!(h.source(), &LoreSource::GenrePack);
    assert_eq!(h.turn_created(), None);
}

#[test]
fn seed_genre_pack_geography_fragment() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let results = store.query_by_category(&LoreCategory::Geography);
    assert_eq!(results.len(), 1);
    let g = &results[0];
    assert_eq!(g.id(), "lore_genre_geography");
    assert!(g.content().contains("broad river valley"));
    assert_eq!(g.source(), &LoreSource::GenrePack);
    assert_eq!(g.turn_created(), None);
}

#[test]
fn seed_genre_pack_cosmology_is_history_category() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let all_history = store.query_by_category(&LoreCategory::History);
    let cosmo = all_history.iter().find(|f| f.id() == "lore_genre_cosmology");
    assert!(cosmo.is_some(), "cosmology should be filed under History");
    assert!(cosmo.unwrap().content().contains("no unified theology"));
}

#[test]
fn seed_genre_pack_factions() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let factions = store.query_by_category(&LoreCategory::Faction);
    assert_eq!(factions.len(), 2, "expected one fragment per faction");
}

#[test]
fn seed_genre_pack_faction_content_includes_name_and_description() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let factions = store.query_by_category(&LoreCategory::Faction);
    let merchant = factions
        .iter()
        .find(|f| f.content().contains("Merchant Consortium"))
        .expect("expected Merchant Consortium faction fragment");
    assert!(merchant.content().contains("wealthy trading families"));
    assert_eq!(merchant.source(), &LoreSource::GenrePack);
    assert_eq!(merchant.turn_created(), None);
}

#[test]
fn seed_genre_pack_faction_metadata_includes_faction_name() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let factions = store.query_by_category(&LoreCategory::Faction);
    let any_faction = &factions[0];
    assert!(
        any_faction.metadata().contains_key("faction_name"),
        "faction fragments should have faction_name in metadata"
    );
}

#[test]
fn seed_genre_pack_returns_fragment_count() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    // history + geography + cosmology + 2 factions = 5
    let count = seed_lore_from_genre_pack(&mut store, &pack);
    assert_eq!(count, 5);
}

#[test]
fn seed_genre_pack_all_sources_are_genre_pack() {
    let lore = test_lore();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    // Check every fragment has GenrePack source
    for cat in &[
        LoreCategory::History,
        LoreCategory::Geography,
        LoreCategory::Faction,
    ] {
        for frag in store.query_by_category(cat) {
            assert_eq!(frag.source(), &LoreSource::GenrePack);
        }
    }
}

// --- Empty section handling ---

#[test]
fn seed_genre_pack_empty_history_skipped() {
    let mut lore = test_lore();
    lore.history = String::new();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let history = store.query_by_category(&LoreCategory::History);
    // Should only have cosmology, not an empty history fragment
    assert!(
        history.iter().all(|f| f.id() != "lore_genre_history"),
        "empty history should not produce a fragment"
    );
}

#[test]
fn seed_genre_pack_empty_geography_skipped() {
    let mut lore = test_lore();
    lore.geography = String::new();
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let geo = store.query_by_category(&LoreCategory::Geography);
    assert!(geo.is_empty(), "empty geography should not produce a fragment");
}

#[test]
fn seed_genre_pack_empty_factions_skipped() {
    let mut lore = test_lore();
    lore.factions = vec![];
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    seed_lore_from_genre_pack(&mut store, &pack);

    let factions = store.query_by_category(&LoreCategory::Faction);
    assert!(factions.is_empty());
}

#[test]
fn seed_genre_pack_all_empty_returns_zero() {
    let lore = Lore {
        world_name: String::new(),
        history: String::new(),
        geography: String::new(),
        cosmology: String::new(),
        factions: vec![],
        extras: HashMap::new(),
    };
    let pack = test_genre_pack(lore, vec![]);
    let mut store = LoreStore::new();
    let count = seed_lore_from_genre_pack(&mut store, &pack);
    assert_eq!(count, 0);
    assert!(store.is_empty());
}

// --- Character creation seeding ---

#[test]
fn seed_char_creation_creates_character_fragments() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    // 2 choices in origins + 1 choice in motivation = 3
    assert_eq!(chars.len(), 3);
}

#[test]
fn seed_char_creation_content_format() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    let hearthlands = chars
        .iter()
        .find(|f| f.content().contains("Hearthlands"))
        .expect("expected Hearthlands fragment");
    assert_eq!(
        hearthlands.content(),
        "The Hearthlands: You grew up on a quiet farm."
    );
}

#[test]
fn seed_char_creation_ids() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    let ids: Vec<&str> = chars.iter().map(|f| f.id()).collect();
    assert!(ids.contains(&"lore_char_creation_origins_0"));
    assert!(ids.contains(&"lore_char_creation_origins_1"));
    assert!(ids.contains(&"lore_char_creation_motivation_0"));
}

#[test]
fn seed_char_creation_source_is_character_creation() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    for frag in store.query_by_category(&LoreCategory::Character) {
        assert_eq!(frag.source(), &LoreSource::CharacterCreation);
    }
}

#[test]
fn seed_char_creation_turn_created_is_none() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    for frag in store.query_by_category(&LoreCategory::Character) {
        assert_eq!(frag.turn_created(), None);
    }
}

#[test]
fn seed_char_creation_metadata_has_scene_id() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    let first = chars
        .iter()
        .find(|f| f.id() == "lore_char_creation_origins_0")
        .unwrap();
    assert_eq!(first.metadata().get("scene_id").unwrap(), "origins");
}

#[test]
fn seed_char_creation_metadata_has_choice_index() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    let second = chars
        .iter()
        .find(|f| f.id() == "lore_char_creation_origins_1")
        .unwrap();
    assert_eq!(second.metadata().get("choice_index").unwrap(), "1");
}

#[test]
fn seed_char_creation_metadata_has_choice_label() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    seed_lore_from_char_creation(&mut store, &scenes);

    let chars = store.query_by_category(&LoreCategory::Character);
    let first = chars
        .iter()
        .find(|f| f.id() == "lore_char_creation_origins_0")
        .unwrap();
    assert_eq!(
        first.metadata().get("choice_label").unwrap(),
        "The Hearthlands"
    );
}

#[test]
fn seed_char_creation_returns_count() {
    let scenes = test_char_creation_scenes();
    let mut store = LoreStore::new();
    let count = seed_lore_from_char_creation(&mut store, &scenes);
    assert_eq!(count, 3);
}

#[test]
fn seed_char_creation_empty_scenes_returns_zero() {
    let mut store = LoreStore::new();
    let count = seed_lore_from_char_creation(&mut store, &[]);
    assert_eq!(count, 0);
    assert!(store.is_empty());
}

#[test]
fn seed_char_creation_scene_with_no_choices_skipped() {
    let scenes = vec![CharCreationScene {
        id: "confirm".to_string(),
        title: "Confirm".to_string(),
        narration: "Confirm your choices.".to_string(),
        choices: vec![],
        allows_freeform: None,
        hook_prompt: None,
        loading_text: None,
        mechanical_effects: None,
    }];
    let mut store = LoreStore::new();
    let count = seed_lore_from_char_creation(&mut store, &scenes);
    assert_eq!(count, 0);
}

// --- Combined tests ---

#[test]
fn seed_combined_genre_and_char_creation() {
    let lore = test_lore();
    let scenes = test_char_creation_scenes();
    let pack = test_genre_pack(lore, scenes.clone());
    let mut store = LoreStore::new();

    let genre_count = seed_lore_from_genre_pack(&mut store, &pack);
    let char_count = seed_lore_from_char_creation(&mut store, &scenes);

    // 5 genre + 3 char creation = 8
    assert_eq!(genre_count + char_count, 8);
    assert_eq!(store.len(), 8);
}

#[test]
fn seed_combined_token_budget_reflects_all() {
    let lore = test_lore();
    let scenes = test_char_creation_scenes();
    let pack = test_genre_pack(lore, scenes.clone());
    let mut store = LoreStore::new();

    seed_lore_from_genre_pack(&mut store, &pack);
    seed_lore_from_char_creation(&mut store, &scenes);

    assert!(
        store.total_tokens() > 0,
        "token budget should be positive after seeding"
    );
}

// ===================================================================
// select_lore_for_prompt tests (story 11-4)
// ===================================================================

/// Helper: build a store with known fragments for injection tests.
fn injection_store() -> LoreStore {
    let mut store = LoreStore::new();
    store
        .add(LoreFragment::new(
            "inj-hist-001".into(),
            LoreCategory::History,
            "The ancient kingdom fell a thousand years ago.".into(),
            LoreSource::GenrePack,
            Some(1),
            HashMap::new(),
        ))
        .unwrap();
    store
        .add(LoreFragment::new(
            "inj-geo-001".into(),
            LoreCategory::Geography,
            "The Crystal Caverns lie beneath the eastern ridge.".into(),
            LoreSource::GenrePack,
            Some(2),
            HashMap::new(),
        ))
        .unwrap();
    store
        .add(LoreFragment::new(
            "inj-fac-001".into(),
            LoreCategory::Faction,
            "The Iron Circle is a secretive merchant guild.".into(),
            LoreSource::GameEvent,
            Some(3),
            HashMap::new(),
        ))
        .unwrap();
    store
}

#[test]
fn select_lore_empty_store_returns_empty() {
    let store = LoreStore::new();
    let result = select_lore_for_prompt(&store, 1000, None, None);
    assert!(result.is_empty());
}

#[test]
fn select_lore_zero_budget_returns_empty() {
    let store = injection_store();
    let result = select_lore_for_prompt(&store, 0, None, None);
    assert!(result.is_empty());
}

#[test]
fn select_lore_large_budget_returns_all() {
    let store = injection_store();
    let result = select_lore_for_prompt(&store, 100_000, None, None);
    assert_eq!(result.len(), 3);
}

#[test]
fn select_lore_respects_token_budget() {
    let store = injection_store();
    // Total tokens across all 3 fragments is well above 10.
    // With a budget of 15, we should not get all fragments.
    let result = select_lore_for_prompt(&store, 15, None, None);
    let total: usize = result.iter().map(|f| f.token_estimate()).sum();
    assert!(total <= 15, "total tokens {total} exceeds budget 15");
    assert!(
        !result.is_empty(),
        "should return at least one fragment for budget 15"
    );
}

#[test]
fn select_lore_no_duplicates() {
    let store = injection_store();
    let result = select_lore_for_prompt(&store, 100_000, None, None);
    let mut ids: Vec<&str> = result.iter().map(|f| f.id()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), result.len(), "duplicate fragments detected");
}

#[test]
fn select_lore_priority_categories_boost_geography() {
    let store = injection_store();
    let cats = [LoreCategory::Geography];
    let result = select_lore_for_prompt(&store, 15, Some(&cats), None);
    let ids: Vec<&str> = result.iter().map(|f| f.id()).collect();
    assert!(
        ids.contains(&"inj-geo-001"),
        "Geography fragment should be prioritized when Geography is priority category"
    );
}

#[test]
fn select_lore_context_hint_still_respects_budget() {
    let mut store = LoreStore::new();
    // One huge fragment matching the hint — larger than budget
    store
        .add(LoreFragment::new(
            "huge".into(),
            LoreCategory::History,
            "dragons ".repeat(500), // ~1000 tokens
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        ))
        .unwrap();
    // One small fragment
    store
        .add(LoreFragment::new(
            "tiny".into(),
            LoreCategory::Geography,
            "A small hill.".into(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        ))
        .unwrap();

    let cats = [LoreCategory::History];
    let result = select_lore_for_prompt(&store, 10, Some(&cats), None);
    let total: usize = result.iter().map(|f| f.token_estimate()).sum();
    assert!(total <= 10, "total tokens {total} exceeds budget 10");
}

#[test]
fn select_lore_prioritises_when_budget_limited() {
    // With a tight budget, the function must choose — it shouldn't just
    // return a random subset. We verify it returns *some* fragments
    // and stays within budget.
    let store = injection_store();
    let budget = store.total_tokens() / 2;
    let result = select_lore_for_prompt(&store, budget, None, None);
    let total: usize = result.iter().map(|f| f.token_estimate()).sum();
    assert!(total <= budget);
    assert!(!result.is_empty());
    assert!(result.len() < 3, "should not fit all fragments");
}

// ===================================================================
// format_lore_context tests (story 11-4)
// ===================================================================

#[test]
fn format_lore_empty_returns_empty_string() {
    let result = format_lore_context(&[]);
    assert!(result.is_empty());
}

#[test]
fn format_lore_single_fragment_includes_content() {
    let frag = LoreFragment::new(
        "fmt-001".into(),
        LoreCategory::History,
        "The kingdom was founded long ago.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let result = format_lore_context(&[&frag]);
    assert!(
        result.contains("The kingdom was founded long ago."),
        "output should contain fragment content"
    );
}

#[test]
fn format_lore_single_fragment_includes_category_header() {
    let frag = LoreFragment::new(
        "fmt-002".into(),
        LoreCategory::Faction,
        "The guild rules the city.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let result = format_lore_context(&[&frag]);
    assert!(
        result.contains("## Faction"),
        "output should contain category header, got: {result}"
    );
}

#[test]
fn format_lore_groups_by_category() {
    let hist1 = LoreFragment::new(
        "fmt-h1".into(),
        LoreCategory::History,
        "Event one.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let hist2 = LoreFragment::new(
        "fmt-h2".into(),
        LoreCategory::History,
        "Event two.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let geo = LoreFragment::new(
        "fmt-g1".into(),
        LoreCategory::Geography,
        "Mountains here.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );

    let result = format_lore_context(&[&hist1, &geo, &hist2]);

    // Both history fragments should appear under the same History header
    let history_section_count = result.matches("## History").count();
    assert_eq!(
        history_section_count, 1,
        "History header should appear exactly once"
    );
    assert!(result.contains("## Geography"));
    assert!(result.contains("Event one."));
    assert!(result.contains("Event two."));
    assert!(result.contains("Mountains here."));
}

#[test]
fn format_lore_multiple_categories_have_headers() {
    let frag_a = LoreFragment::new(
        "fmt-a".into(),
        LoreCategory::History,
        "Ancient times.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let frag_b = LoreFragment::new(
        "fmt-b".into(),
        LoreCategory::Faction,
        "The council.".into(),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    );
    let frag_c = LoreFragment::new(
        "fmt-c".into(),
        LoreCategory::Character,
        "A brave hero.".into(),
        LoreSource::CharacterCreation,
        None,
        HashMap::new(),
    );

    let result = format_lore_context(&[&frag_a, &frag_b, &frag_c]);
    assert!(result.contains("## History"));
    assert!(result.contains("## Faction"));
    assert!(result.contains("## Character"));
}

// ===================================================================
// Lore accumulation tests (story 11-5)
// ===================================================================

#[test]
fn accumulate_lore_creates_fragment_with_game_event_source() {
    let mut store = LoreStore::new();
    let id = accumulate_lore(
        &mut store,
        "The hero defeated the dragon",
        LoreCategory::Event,
        5,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_keyword("defeated the dragon");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source(), &LoreSource::GameEvent);
    assert_eq!(results[0].id(), id);
}

#[test]
fn accumulate_lore_sets_turn_created() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "A mysterious stranger arrived",
        LoreCategory::Character,
        42,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_keyword("mysterious stranger");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].turn_created(), Some(42));
}

#[test]
fn accumulate_lore_content_matches_description() {
    let mut store = LoreStore::new();
    let desc = "The ancient temple crumbled to dust";
    accumulate_lore(
        &mut store,
        desc,
        LoreCategory::History,
        10,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_keyword("ancient temple");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content(), desc);
}

#[test]
fn accumulate_lore_category_history() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "The kingdom fell centuries ago",
        LoreCategory::History,
        1,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_category(&LoreCategory::History);
    assert_eq!(results.len(), 1);
}

#[test]
fn accumulate_lore_category_faction() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "The Shadow Guild gained influence",
        LoreCategory::Faction,
        3,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_category(&LoreCategory::Faction);
    assert_eq!(results.len(), 1);
}

#[test]
fn accumulate_lore_category_geography() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "A new mountain pass was discovered",
        LoreCategory::Geography,
        7,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_category(&LoreCategory::Geography);
    assert_eq!(results.len(), 1);
}

#[test]
fn accumulate_lore_category_custom() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "A prophecy was revealed",
        LoreCategory::Custom("Prophecy".to_string()),
        2,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_category(&LoreCategory::Custom("Prophecy".to_string()));
    assert_eq!(results.len(), 1);
}

#[test]
fn accumulate_lore_preserves_metadata() {
    let mut store = LoreStore::new();
    let mut meta = HashMap::new();
    meta.insert("event_type".to_string(), "combat".to_string());
    meta.insert("location".to_string(), "dark_forest".to_string());

    accumulate_lore(
        &mut store,
        "A battle erupted in the forest",
        LoreCategory::Event,
        15,
        meta,
    )
    .unwrap();

    let results = store.query_by_keyword("battle erupted");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].metadata().get("event_type").unwrap(),
        "combat"
    );
    assert_eq!(
        results[0].metadata().get("location").unwrap(),
        "dark_forest"
    );
}

#[test]
fn accumulate_lore_computes_token_estimate() {
    let mut store = LoreStore::new();
    let desc = "a]".repeat(20); // 40 chars → 10 tokens
    accumulate_lore(
        &mut store,
        &desc,
        LoreCategory::Event,
        1,
        HashMap::new(),
    )
    .unwrap();

    let results = store.query_by_keyword(&desc);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].token_estimate(), 10);
}

#[test]
fn accumulate_lore_increases_store_len() {
    let mut store = LoreStore::new();
    assert_eq!(store.len(), 0);

    accumulate_lore(
        &mut store,
        "Something happened",
        LoreCategory::Event,
        1,
        HashMap::new(),
    )
    .unwrap();
    assert_eq!(store.len(), 1);

    accumulate_lore(
        &mut store,
        "Something else happened",
        LoreCategory::Event,
        2,
        HashMap::new(),
    )
    .unwrap();
    assert_eq!(store.len(), 2);
}

#[test]
fn accumulate_lore_returns_fragment_id() {
    let mut store = LoreStore::new();
    let id = accumulate_lore(
        &mut store,
        "The king abdicated the throne",
        LoreCategory::History,
        8,
        HashMap::new(),
    )
    .unwrap();

    // Id should be non-empty
    assert!(!id.is_empty());
    // The fragment should be findable by keyword and have this id
    let results = store.query_by_keyword("king abdicated");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id(), id);
}

#[test]
fn accumulate_lore_unique_ids_same_turn_different_content() {
    let mut store = LoreStore::new();
    let id1 = accumulate_lore(
        &mut store,
        "The hero found a sword",
        LoreCategory::Item,
        5,
        HashMap::new(),
    )
    .unwrap();
    let id2 = accumulate_lore(
        &mut store,
        "The hero found a shield",
        LoreCategory::Item,
        5,
        HashMap::new(),
    )
    .unwrap();

    assert_ne!(id1, id2);
    assert_eq!(store.len(), 2);
}

#[test]
fn accumulate_lore_rejects_empty_description() {
    let mut store = LoreStore::new();
    let result = accumulate_lore(
        &mut store,
        "",
        LoreCategory::Event,
        1,
        HashMap::new(),
    );

    assert!(result.is_err());
    assert_eq!(store.len(), 0);
}

#[test]
fn accumulate_lore_queryable_by_category_after_add() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "New trade routes opened to the east",
        LoreCategory::Geography,
        20,
        HashMap::new(),
    )
    .unwrap();

    let by_cat = store.query_by_category(&LoreCategory::Geography);
    assert_eq!(by_cat.len(), 1);
    assert_eq!(by_cat[0].content(), "New trade routes opened to the east");
}

#[test]
fn accumulate_lore_queryable_by_keyword_after_add() {
    let mut store = LoreStore::new();
    accumulate_lore(
        &mut store,
        "The wizard enchanted a powerful amulet",
        LoreCategory::Item,
        11,
        HashMap::new(),
    )
    .unwrap();

    let by_kw = store.query_by_keyword("enchanted");
    assert_eq!(by_kw.len(), 1);
    assert_eq!(by_kw[0].turn_created(), Some(11));
}

// --- accumulate_lore_batch tests ---

#[test]
fn accumulate_lore_batch_processes_multiple_events() {
    let mut store = LoreStore::new();
    let events = vec![
        (
            "A village was founded".to_string(),
            LoreCategory::History,
            1,
            HashMap::new(),
        ),
        (
            "A river was discovered".to_string(),
            LoreCategory::Geography,
            2,
            HashMap::new(),
        ),
        (
            "Two factions clashed".to_string(),
            LoreCategory::Faction,
            3,
            HashMap::new(),
        ),
    ];

    let results = accumulate_lore_batch(&mut store, &events);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_ok()));
    assert_eq!(store.len(), 3);
}

#[test]
fn accumulate_lore_batch_returns_errors_for_empty_descriptions() {
    let mut store = LoreStore::new();
    let events = vec![
        (
            "Valid event".to_string(),
            LoreCategory::Event,
            1,
            HashMap::new(),
        ),
        (
            "".to_string(),
            LoreCategory::Event,
            2,
            HashMap::new(),
        ),
        (
            "Another valid event".to_string(),
            LoreCategory::Event,
            3,
            HashMap::new(),
        ),
    ];

    let results = accumulate_lore_batch(&mut store, &events);
    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_ok());
    // Only 2 valid fragments added
    assert_eq!(store.len(), 2);
}

#[test]
fn accumulate_lore_batch_empty_input() {
    let mut store = LoreStore::new();
    let results = accumulate_lore_batch(&mut store, &[]);
    assert!(results.is_empty());
    assert_eq!(store.len(), 0);
}

// ===================================================================
// Embedding field on LoreFragment (story 11-6)
// ===================================================================

#[test]
fn new_fragment_has_no_embedding() {
    let frag = sample_fragment();
    assert!(frag.embedding().is_none());
}

#[test]
fn with_embedding_stores_vector() {
    let frag = sample_fragment().with_embedding(vec![1.0, 2.0, 3.0]);
    let emb = frag.embedding().expect("should have embedding");
    assert_eq!(emb, &[1.0, 2.0, 3.0]);
}

#[test]
fn with_embedding_preserves_other_fields() {
    let frag = sample_fragment().with_embedding(vec![0.5]);
    assert_eq!(frag.id(), "lore-001");
    assert_eq!(frag.category(), &LoreCategory::History);
    assert_eq!(frag.content(), "The Flickering Reach was once a thriving trade hub.");
}

#[test]
fn serde_round_trip_without_embedding() {
    let frag = sample_fragment();
    let json = serde_json::to_string(&frag).expect("serialize");
    // embedding field should be absent from JSON
    assert!(!json.contains("embedding"));
    let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
    assert!(restored.embedding().is_none());
}

#[test]
fn serde_round_trip_with_embedding() {
    let frag = sample_fragment().with_embedding(vec![0.1, 0.2, 0.3]);
    let json = serde_json::to_string(&frag).expect("serialize");
    let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
    let emb = restored.embedding().expect("should have embedding");
    assert_eq!(emb.len(), 3);
    assert!((emb[0] - 0.1).abs() < 1e-6);
    assert!((emb[1] - 0.2).abs() < 1e-6);
    assert!((emb[2] - 0.3).abs() < 1e-6);
}

// ===================================================================
// Cosine similarity (story 11-6)
// ===================================================================

#[test]
fn cosine_similarity_identical_vectors() {
    let v = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&v, &v);
    assert!((sim - 1.0).abs() < 1e-6, "identical vectors should be 1.0, got {sim}");
}

#[test]
fn cosine_similarity_orthogonal_vectors() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let sim = cosine_similarity(&a, &b);
    assert!(sim.abs() < 1e-6, "orthogonal vectors should be 0.0, got {sim}");
}

#[test]
fn cosine_similarity_opposite_vectors() {
    let a = vec![1.0, 0.0];
    let b = vec![-1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - (-1.0)).abs() < 1e-6, "opposite vectors should be -1.0, got {sim}");
}

#[test]
fn cosine_similarity_different_lengths_returns_zero() {
    let a = vec![1.0, 2.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 0.0).abs() < 1e-6, "mismatched lengths should be 0.0, got {sim}");
}

#[test]
fn cosine_similarity_zero_vector_returns_zero() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 0.0).abs() < 1e-6, "zero vector should give 0.0, got {sim}");
}

#[test]
fn cosine_similarity_empty_vectors_returns_zero() {
    let sim = cosine_similarity(&[], &[]);
    assert!((sim - 0.0).abs() < 1e-6, "empty vectors should give 0.0, got {sim}");
}

#[test]
fn cosine_similarity_scaled_vectors_are_identical() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![2.0, 4.0, 6.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 1.0).abs() < 1e-6, "scaled vectors should be 1.0, got {sim}");
}

// ===================================================================
// query_by_similarity on LoreStore (story 11-6)
// ===================================================================

fn make_fragment_with_embedding(id: &str, embedding: Vec<f32>) -> LoreFragment {
    LoreFragment::new(
        id.to_string(),
        LoreCategory::History,
        format!("Content for {id}"),
        LoreSource::GenrePack,
        None,
        HashMap::new(),
    )
    .with_embedding(embedding)
}

#[test]
fn query_by_similarity_empty_store() {
    let store = LoreStore::new();
    let results = store.query_by_similarity(&[1.0, 0.0], 5);
    assert!(results.is_empty());
}

#[test]
fn query_by_similarity_no_embeddings_returns_empty() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    let results = store.query_by_similarity(&[1.0, 0.0], 5);
    assert!(results.is_empty());
}

#[test]
fn query_by_similarity_returns_sorted_by_score() {
    let mut store = LoreStore::new();
    // query is [1,0] — frag_a=[1,0] is perfect match, frag_b=[0,1] is orthogonal
    store.add(make_fragment_with_embedding("sim-a", vec![1.0, 0.0])).unwrap();
    store.add(make_fragment_with_embedding("sim-b", vec![0.0, 1.0])).unwrap();
    store.add(make_fragment_with_embedding("sim-c", vec![0.7, 0.7])).unwrap();

    let results = store.query_by_similarity(&[1.0, 0.0], 10);
    assert_eq!(results.len(), 3);
    // First result should be the most similar (sim-a)
    assert_eq!(results[0].0.id(), "sim-a");
    // Scores should be descending
    assert!(results[0].1 >= results[1].1);
    assert!(results[1].1 >= results[2].1);
}

#[test]
fn query_by_similarity_respects_top_k() {
    let mut store = LoreStore::new();
    store.add(make_fragment_with_embedding("tk-a", vec![1.0, 0.0])).unwrap();
    store.add(make_fragment_with_embedding("tk-b", vec![0.5, 0.5])).unwrap();
    store.add(make_fragment_with_embedding("tk-c", vec![0.0, 1.0])).unwrap();

    let results = store.query_by_similarity(&[1.0, 0.0], 2);
    assert_eq!(results.len(), 2);
}

#[test]
fn query_by_similarity_skips_fragments_without_embeddings() {
    let mut store = LoreStore::new();
    // One with embedding, one without
    store.add(make_fragment_with_embedding("with-emb", vec![1.0, 0.0])).unwrap();
    store.add(history_fragment()).unwrap(); // no embedding

    let results = store.query_by_similarity(&[1.0, 0.0], 10);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.id(), "with-emb");
}

#[test]
fn query_by_similarity_mixed_embeddings_only_returns_embedded() {
    let mut store = LoreStore::new();
    store.add(make_fragment_with_embedding("e1", vec![1.0, 0.0])).unwrap();
    store.add(geography_fragment()).unwrap(); // no embedding
    store.add(make_fragment_with_embedding("e2", vec![0.0, 1.0])).unwrap();
    store.add(faction_fragment()).unwrap(); // no embedding

    let results = store.query_by_similarity(&[0.5, 0.5], 10);
    assert_eq!(results.len(), 2);
    // Both returned fragments should have embeddings
    for (frag, _score) in &results {
        assert!(frag.embedding().is_some());
    }
}

// ===================================================================
// Graceful fallback — existing behavior unchanged (story 11-6)
// ===================================================================

#[test]
fn select_lore_for_prompt_still_works_without_embeddings() {
    let mut store = LoreStore::new();
    store.add(history_fragment()).unwrap();
    store.add(geography_fragment()).unwrap();
    // None of these have embeddings — select_lore_for_prompt should still work
    let selected = select_lore_for_prompt(&store, 1000, None, None);
    assert_eq!(selected.len(), 2);
}

// ===================================================================
// Language knowledge — bridge conlang ↔ lore (story 11-10)
// ===================================================================

use crate::conlang::{GeneratedName, MorphemeCategory, NamePattern};

fn sample_morpheme(morpheme: &str, meaning: &str, language_id: &str) -> Morpheme {
    Morpheme {
        morpheme: morpheme.to_string(),
        meaning: meaning.to_string(),
        pronunciation_hint: None,
        category: MorphemeCategory::Root,
        language_id: language_id.to_string(),
    }
}

fn sample_generated_name() -> GeneratedName {
    GeneratedName {
        name: "zar'thi".to_string(),
        gloss: "fire-one who".to_string(),
        pronunciation: Some("zahr'thee".to_string()),
        pattern: NamePattern::RootSuffix,
        language_id: "draconic".to_string(),
    }
}

// --- record_language_knowledge ---

#[test]
fn record_language_knowledge_creates_language_category() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    let id = record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags.len(), 1);
    assert_eq!(frags[0].id(), id);
    assert_eq!(frags[0].category(), &LoreCategory::Language);
}

#[test]
fn record_language_knowledge_source_is_game_event() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    let id = record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    let frag = frags.iter().find(|f| f.id() == id).unwrap();
    assert_eq!(frag.source(), &LoreSource::GameEvent);
}

#[test]
fn record_language_knowledge_sets_turn_created() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 7).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].turn_created(), Some(7));
}

#[test]
fn record_language_knowledge_content_includes_morpheme_and_meaning() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    let content = frags[0].content();
    assert!(content.contains("zar"), "Content should include morpheme string");
    assert!(content.contains("fire"), "Content should include meaning");
}

#[test]
fn record_language_knowledge_metadata_has_character_id() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("character_id").unwrap(), "char-1");
}

#[test]
fn record_language_knowledge_metadata_has_language_id() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("language_id").unwrap(), "draconic");
}

#[test]
fn record_language_knowledge_metadata_has_morpheme() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("morpheme").unwrap(), "zar");
}

#[test]
fn record_language_knowledge_metadata_has_meaning() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("meaning").unwrap(), "fire");
}

#[test]
fn record_language_knowledge_adds_fragment_to_store() {
    let mut store = LoreStore::new();
    assert_eq!(store.len(), 0);
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    assert_eq!(store.len(), 1);
}

#[test]
fn record_language_knowledge_returns_fragment_id() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    let id = record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    assert!(!id.is_empty(), "Returned id should not be empty");
}

// --- record_name_knowledge ---

#[test]
fn record_name_knowledge_creates_language_fragment() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    let id = record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags.len(), 1);
    assert_eq!(frags[0].id(), id);
    assert_eq!(frags[0].category(), &LoreCategory::Language);
}

#[test]
fn record_name_knowledge_content_includes_name_and_gloss() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    let content = frags[0].content();
    assert!(content.contains("zar'thi"), "Content should include the name");
    assert!(content.contains("fire-one who"), "Content should include the gloss");
}

#[test]
fn record_name_knowledge_metadata_has_character_id() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("character_id").unwrap(), "char-1");
}

#[test]
fn record_name_knowledge_metadata_has_language_id() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("language_id").unwrap(), "draconic");
}

#[test]
fn record_name_knowledge_metadata_has_name() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("name").unwrap(), "zar'thi");
}

#[test]
fn record_name_knowledge_metadata_has_gloss() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    let frags = store.query_by_category(&LoreCategory::Language);
    assert_eq!(frags[0].metadata().get("gloss").unwrap(), "fire-one who");
}

#[test]
fn record_name_knowledge_returns_fragment_id() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    let id = record_name_knowledge(&mut store, &name, "char-1", 10).unwrap();
    assert!(!id.is_empty());
}

// --- query_language_knowledge ---

#[test]
fn query_language_knowledge_returns_matching_fragments() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 1);
}

#[test]
fn query_language_knowledge_empty_for_unknown_character() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let results = query_language_knowledge(&store, "char-unknown", "draconic");
    assert!(results.is_empty());
}

#[test]
fn query_language_knowledge_empty_for_unknown_language() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    let results = query_language_knowledge(&store, "char-1", "elvish");
    assert!(results.is_empty());
}

#[test]
fn query_language_knowledge_multiple_words_returned() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("dra", "dragon", "draconic");
    record_language_knowledge(&mut store, &m1, "char-1", 5).unwrap();
    record_language_knowledge(&mut store, &m2, "char-1", 6).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 2);
}

#[test]
fn query_language_knowledge_excludes_other_characters() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 5).unwrap();
    record_language_knowledge(&mut store, &sample_morpheme("dra", "dragon", "draconic"), "char-2", 6).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 1);
}

#[test]
fn query_language_knowledge_excludes_other_languages() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("ael", "star", "elvish");
    record_language_knowledge(&mut store, &m1, "char-1", 5).unwrap();
    record_language_knowledge(&mut store, &m2, "char-1", 6).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 1);
}

// --- Integration ---

#[test]
fn integration_record_multiple_query_returns_all() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("dra", "dragon", "draconic");
    let m3 = sample_morpheme("kel", "stone", "draconic");
    record_language_knowledge(&mut store, &m1, "char-1", 1).unwrap();
    record_language_knowledge(&mut store, &m2, "char-1", 2).unwrap();
    record_language_knowledge(&mut store, &m3, "char-1", 3).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 3);
}

#[test]
fn integration_mixed_morpheme_and_name_knowledge_queryable() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    let name = sample_generated_name();
    record_language_knowledge(&mut store, &m, "char-1", 1).unwrap();
    record_name_knowledge(&mut store, &name, "char-1", 2).unwrap();
    let results = query_language_knowledge(&store, "char-1", "draconic");
    assert_eq!(results.len(), 2);
}

// --- query_all_language_knowledge (story 15-19) ---

#[test]
fn query_all_language_knowledge_returns_all_languages() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("ael", "star", "elvish");
    record_language_knowledge(&mut store, &m1, "char-1", 1).unwrap();
    record_language_knowledge(&mut store, &m2, "char-1", 2).unwrap();
    let results = query_all_language_knowledge(&store, "char-1");
    assert_eq!(results.len(), 2);
}

#[test]
fn query_all_language_knowledge_excludes_other_characters() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("dra", "dragon", "draconic");
    record_language_knowledge(&mut store, &m1, "char-1", 1).unwrap();
    record_language_knowledge(&mut store, &m2, "char-2", 2).unwrap();
    let results = query_all_language_knowledge(&store, "char-1");
    assert_eq!(results.len(), 1);
}

#[test]
fn query_all_language_knowledge_empty_store_returns_empty() {
    let store = LoreStore::new();
    let results = query_all_language_knowledge(&store, "char-1");
    assert!(results.is_empty());
}

// --- format_language_knowledge_for_prompt (story 15-19) ---

#[test]
fn format_language_knowledge_empty_returns_empty_string() {
    let result = format_language_knowledge_for_prompt(&[]);
    assert_eq!(result, "");
}

#[test]
fn format_language_knowledge_includes_morpheme_and_meaning() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 1).unwrap();
    let frags = query_all_language_knowledge(&store, "char-1");
    let result = format_language_knowledge_for_prompt(&frags);
    assert!(result.contains("zar"), "Should contain morpheme 'zar'");
    assert!(result.contains("fire"), "Should contain meaning 'fire'");
}

#[test]
fn format_language_knowledge_includes_name_and_gloss() {
    let mut store = LoreStore::new();
    let name = sample_generated_name();
    record_name_knowledge(&mut store, &name, "char-1", 1).unwrap();
    let frags = query_all_language_knowledge(&store, "char-1");
    let result = format_language_knowledge_for_prompt(&frags);
    assert!(result.contains(&name.name), "Should contain name");
    assert!(result.contains(&name.gloss), "Should contain gloss");
}

#[test]
fn format_language_knowledge_groups_by_language() {
    let mut store = LoreStore::new();
    let m1 = sample_morpheme("zar", "fire", "draconic");
    let m2 = sample_morpheme("ael", "star", "elvish");
    record_language_knowledge(&mut store, &m1, "char-1", 1).unwrap();
    record_language_knowledge(&mut store, &m2, "char-1", 2).unwrap();
    let frags = query_all_language_knowledge(&store, "char-1");
    let result = format_language_knowledge_for_prompt(&frags);
    assert!(
        result.contains("draconic"),
        "Should contain language 'draconic'"
    );
    assert!(
        result.contains("elvish"),
        "Should contain language 'elvish'"
    );
}

#[test]
fn format_language_knowledge_has_header() {
    let mut store = LoreStore::new();
    let m = sample_morpheme("zar", "fire", "draconic");
    record_language_knowledge(&mut store, &m, "char-1", 1).unwrap();
    let frags = query_all_language_knowledge(&store, "char-1");
    let result = format_language_knowledge_for_prompt(&frags);
    assert!(
        result.contains("CONSTRUCTED LANGUAGE VOCABULARY"),
        "Should have section header"
    );
}
