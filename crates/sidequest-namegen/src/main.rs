//! NPC identity generator binary.
//!
//! Generates a complete NPC block from genre pack data: culture-appropriate name
//! (Markov chains + mad-libs patterns), archetype personality, OCEAN profile,
//! dialogue quirks, inventory hints, and trope connections.
//!
//! Called by the narrator agent when introducing new NPCs.
//!
//! Usage:
//!   sidequest-namegen --genre-packs-path ./genre_packs --genre mutant_wasteland
//!   sidequest-namegen --genre-packs-path ./genre_packs --genre mutant_wasteland --culture Scrapborn --gender female
//!   sidequest-namegen --genre-packs-path ./genre_packs --genre mutant_wasteland --archetype "Wasteland Trader" --role mechanic

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use rand::Rng;
use serde::Serialize;
use sidequest_genre::{load_genre_pack, GenrePack, NpcArchetype, OceanProfile};

#[derive(Parser)]
#[command(name = "sidequest-namegen", about = "Generate a complete NPC identity from genre pack data")]
struct Cli {
    /// Path to the genre_packs/ directory. Also reads SIDEQUEST_CONTENT_PATH env var.
    #[arg(long, env = "SIDEQUEST_CONTENT_PATH")]
    genre_packs_path: PathBuf,

    /// Genre slug (e.g., mutant_wasteland). Also reads SIDEQUEST_GENRE env var.
    #[arg(long, env = "SIDEQUEST_GENRE")]
    genre: String,

    /// Culture name (e.g., Scrapborn). Random if omitted.
    #[arg(long)]
    culture: Option<String>,

    /// Archetype name (e.g., "Wasteland Trader"). Random if omitted.
    #[arg(long)]
    archetype: Option<String>,

    /// Gender: male, female, nonbinary. Random if omitted.
    #[arg(long)]
    gender: Option<String>,

    /// Role override (e.g., mechanic). Defaults to archetype name.
    #[arg(long)]
    role: Option<String>,

    /// Physical description hints to layer on top of archetype.
    #[arg(long)]
    description: Option<String>,
}

#[derive(Serialize)]
struct NpcBlock {
    name: String,
    pronouns: String,
    gender: String,
    culture: String,
    faction: String,
    faction_description: String,
    archetype: String,
    role: String,
    appearance: String,
    personality: Vec<String>,
    dialogue_quirks: Vec<String>,
    history: String,
    ocean: OceanValues,
    ocean_summary: String,
    disposition: i32,
    inventory: Vec<String>,
    stat_ranges: HashMap<String, [i32; 2]>,
    trope_connections: Vec<TropeConnection>,
}

#[derive(Serialize)]
struct OceanValues {
    openness: f64,
    conscientiousness: f64,
    extraversion: f64,
    agreeableness: f64,
    neuroticism: f64,
}

#[derive(Serialize)]
struct TropeConnection {
    trope: String,
    category: String,
    connection: String,
}

fn main() {
    let cli = Cli::parse();

    let genre_dir = cli.genre_packs_path.join(&cli.genre);
    let pack = match load_genre_pack(&genre_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error loading genre pack: {e}");
            std::process::exit(1);
        }
    };

    let mut rng = rand::rng();
    let npc = generate_npc(&pack, &genre_dir, &cli, &mut rng);

    let json = serde_json::to_string_pretty(&npc).unwrap();
    println!("{json}");

    // Write sidecar JSONL so the orchestrator can see this tool was called.
    // The orchestrator passes SIDEQUEST_TOOL_SIDECAR_DIR and SIDEQUEST_TOOL_SESSION_ID
    // as env vars when invoking Claude CLI with tools.
    write_sidecar(&npc);
}

/// Write a tool call record to the sidecar JSONL file for the orchestrator.
fn write_sidecar(npc: &NpcBlock) {
    let dir = match std::env::var("SIDEQUEST_TOOL_SIDECAR_DIR") {
        Ok(d) => d,
        Err(_) => return, // Not running under orchestrator — skip silently
    };
    let session_id = match std::env::var("SIDEQUEST_TOOL_SESSION_ID") {
        Ok(s) => s,
        Err(_) => return,
    };

    let sidecar_path = std::path::PathBuf::from(&dir)
        .join(format!("sidequest-tools-{session_id}.jsonl"));

    // Ensure directory exists
    let _ = std::fs::create_dir_all(&dir);

    // Write personality_event record for the NPC
    let record = serde_json::json!({
        "tool": "personality_event",
        "result": {
            "npc": &npc.name,
            "event_type": "introduced",
            "description": format!("{} ({})", &npc.role, &npc.archetype)
        }
    });

    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&sidecar_path)
    {
        let _ = writeln!(f, "{}", serde_json::to_string(&record).unwrap());
    }
}

fn generate_npc(pack: &GenrePack, genre_dir: &std::path::Path, cli: &Cli, rng: &mut impl Rng) -> NpcBlock {
    let corpus_dir = genre_dir.join("corpus");

    // Select culture
    let culture = if let Some(ref name) = cli.culture {
        pack.cultures
            .iter()
            .find(|c| c.name.as_str().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                eprintln!("Culture '{}' not found. Available: {}", name,
                    pack.cultures.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
                std::process::exit(1);
            })
    } else {
        &pack.cultures[rng.random_range(0..pack.cultures.len())]
    };

    // Select archetype
    let archetype = if let Some(ref name) = cli.archetype {
        pack.archetypes
            .iter()
            .find(|a| a.name.as_str().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                eprintln!("Archetype '{}' not found. Available: {}", name,
                    pack.archetypes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
                std::process::exit(1);
            })
    } else {
        &pack.archetypes[rng.random_range(0..pack.archetypes.len())]
    };

    // Generate name
    let result = sidequest_genre::names::build_from_culture(culture, &corpus_dir, rng);
    let mut name = String::new();
    for _ in 0..10 {
        let candidate = result.generator.generate_person(rng);
        if !candidate.is_empty()
            && !candidate.to_lowercase().starts_with("of ")
            && !candidate.to_lowercase().starts_with("the ")
        {
            name = candidate;
            break;
        }
    }
    if name.is_empty() {
        name = result.generator.generate_person(rng);
    }

    // Gender + pronouns
    let gender = cli.gender.clone().unwrap_or_else(|| {
        ["male", "female", "nonbinary"][rng.random_range(0..3)].to_string()
    });
    let pronouns = match gender.as_str() {
        "male" => "he/him",
        "female" => "she/her",
        _ => "they/them",
    }.to_string();

    // OCEAN personality (jittered from archetype baseline)
    let ocean = jitter_ocean(archetype, rng);
    let ocean_summary = summarize_ocean(&ocean);

    // Role
    let role = cli.role.clone().unwrap_or_else(|| archetype.name.as_str().to_lowercase());

    // Appearance
    let mut appearance = String::new();
    if let Some(ref desc) = cli.description {
        appearance.push_str(desc);
        appearance.push_str(". ");
    }
    appearance.push_str(&archetype.description);

    // History
    let history = generate_history(culture.name.as_str(), &role, archetype, rng);

    // Trope connections
    let trope_connections = match_tropes(&pack.tropes, archetype, culture);

    NpcBlock {
        name,
        pronouns,
        gender,
        culture: culture.name.as_str().to_string(),
        faction: culture.name.as_str().to_string(),
        faction_description: culture.description.clone(),
        archetype: archetype.name.as_str().to_string(),
        role,
        appearance,
        personality: archetype.personality_traits.clone(),
        dialogue_quirks: archetype.dialogue_quirks.clone(),
        history,
        ocean,
        ocean_summary,
        disposition: archetype.disposition_default,
        inventory: archetype.inventory_hints.clone(),
        stat_ranges: archetype.stat_ranges.clone(),
        trope_connections,
    }
}

fn jitter_ocean(archetype: &NpcArchetype, rng: &mut impl Rng) -> OceanValues {
    let base = archetype.ocean.as_ref().map(|o| {
        (o.openness, o.conscientiousness, o.extraversion, o.agreeableness, o.neuroticism)
    }).unwrap_or((5.0, 5.0, 5.0, 5.0, 5.0));

    let j = |v: f64, rng: &mut dyn rand::RngCore| -> f64 {
        let jitter: f64 = rng.random_range(-1.5..1.5);
        ((v + jitter).clamp(0.0, 10.0) * 10.0).round() / 10.0
    };

    OceanValues {
        openness: j(base.0, rng),
        conscientiousness: j(base.1, rng),
        extraversion: j(base.2, rng),
        agreeableness: j(base.3, rng),
        neuroticism: j(base.4, rng),
    }
}

fn summarize_ocean(o: &OceanValues) -> String {
    fn label(v: f64, low: &str, mid: &str, high: &str) -> String {
        if v < 4.0 { low.to_string() } else if v > 7.0 { high.to_string() } else { mid.to_string() }
    }

    [
        label(o.openness, "conventional and practical", "balanced between tradition and novelty", "curious and imaginative"),
        label(o.conscientiousness, "spontaneous and flexible", "moderately organized", "meticulous and disciplined"),
        label(o.extraversion, "reserved and quiet", "selectively social", "outgoing and talkative"),
        label(o.agreeableness, "blunt and competitive", "pragmatic", "warm and cooperative"),
        label(o.neuroticism, "emotionally steady", "occasionally anxious", "easily stressed and reactive"),
    ].join(", ")
}

const HISTORY_TEMPLATES: &[&str] = &[
    "Once served as a {role} in {faction} territory before {event}.",
    "Grew up in the {faction} settlements. Left after {event}.",
    "Claims to have been {role} for years, but something about the story doesn't add up.",
    "Arrived from the wastes with nothing. Earned {faction} trust through {deed}.",
    "Former {alt_role} who switched trades after {event}.",
    "Born into {faction} culture. Never left the region. Knows every path and every grudge.",
];

const HISTORY_EVENTS: &[&str] = &[
    "a bad trade went wrong", "their settlement was raided", "a mutation changed everything",
    "a drought drove them out", "they found something in the ruins they won't talk about",
    "a feud with another faction", "the water turned bad", "they lost someone important",
    "an Ancient device activated nearby", "a pack of beasts destroyed their homestead",
];

const HISTORY_DEEDS: &[&str] = &[
    "hard work and silence", "a timely warning about raiders", "fixing something nobody else could",
    "sharing water during the drought", "standing their ground when it mattered",
    "knowing where the good salvage was", "patching up the wounded after the last raid",
];

fn generate_history(faction: &str, role: &str, archetype: &NpcArchetype, rng: &mut impl Rng) -> String {
    let template = HISTORY_TEMPLATES[rng.random_range(0..HISTORY_TEMPLATES.len())];
    let event = HISTORY_EVENTS[rng.random_range(0..HISTORY_EVENTS.len())];
    let deed = HISTORY_DEEDS[rng.random_range(0..HISTORY_DEEDS.len())];
    let alt_role = archetype.typical_classes.first().map(|s| s.to_lowercase()).unwrap_or_else(|| "drifter".to_string());

    template
        .replace("{role}", role)
        .replace("{faction}", faction)
        .replace("{event}", event)
        .replace("{deed}", deed)
        .replace("{alt_role}", &alt_role)
}

fn match_tropes(
    tropes: &[sidequest_genre::TropeDefinition],
    archetype: &NpcArchetype,
    culture: &sidequest_genre::Culture,
) -> Vec<TropeConnection> {
    let mut npc_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cls in &archetype.typical_classes {
        npc_tags.insert(cls.to_lowercase());
    }
    for trait_word in &archetype.personality_traits {
        npc_tags.insert(trait_word.to_lowercase());
    }
    npc_tags.insert(culture.name.as_str().to_lowercase());
    for word in archetype.name.as_str().to_lowercase().split_whitespace() {
        npc_tags.insert(word.to_string());
    }

    tropes
        .iter()
        .filter_map(|trope| {
            let trope_tags: std::collections::HashSet<String> =
                trope.tags.iter().map(|t| t.to_lowercase()).collect();
            let overlap: Vec<String> = npc_tags.intersection(&trope_tags).cloned().collect();
            if overlap.is_empty() {
                None
            } else {
                Some(TropeConnection {
                    trope: trope.name.to_string(),
                    category: trope.category.clone(),
                    connection: format!("linked via: {}", overlap.join(", ")),
                })
            }
        })
        .collect()
}
