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
use sidequest_genre::archetype::{resolve_archetype, ResolutionSource};
use sidequest_genre::models::archetype_constraints::ArchetypeConstraints;
use sidequest_genre::models::archetype_funnels::ArchetypeFunnels;
use sidequest_genre::models::npc_traits::NpcTrait;
use sidequest_genre::{load_genre_pack, GenrePack, NpcArchetype};

#[derive(Parser)]
#[command(
    name = "sidequest-namegen",
    about = "Generate a complete NPC identity from genre pack data"
)]
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

    /// Jungian archetype axis (e.g. sage, hero, outlaw). Random if omitted.
    #[arg(long)]
    jungian: Option<String>,

    /// RPG role axis (e.g. healer, tank, stealth). Random if omitted.
    #[arg(long)]
    rpg_role: Option<String>,

    /// NPC narrative role (e.g. mentor, mook, authority). Random if omitted.
    #[arg(long)]
    npc_role: Option<String>,

    /// World slug for funnel resolution. If omitted, skips world funnels.
    #[arg(long)]
    world: Option<String>,
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
    jungian_id: String,
    rpg_role_id: String,
    npc_role_id: Option<String>,
    resolved_archetype: String,
    resolution_source: String,
    /// Spawn-tier quirks selected from the NPC traits database.
    spawn_quirks: Vec<String>,
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

    let sidecar_path =
        std::path::PathBuf::from(&dir).join(format!("sidequest-tools-{session_id}.jsonl"));

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

/// Select a [jungian, rpg_role] pair from the constraints file using weighted randomness.
/// Weights: 60% common, 30% uncommon, 10% rare.
fn select_weighted_pairing(
    constraints: &ArchetypeConstraints,
    rng: &mut impl Rng,
) -> (String, String) {
    let roll: f64 = rng.random_range(0.0..1.0);
    let (pool, fallbacks) = if roll < 0.6 {
        (
            &constraints.valid_pairings.common,
            vec![
                &constraints.valid_pairings.uncommon,
                &constraints.valid_pairings.rare,
            ],
        )
    } else if roll < 0.9 {
        (
            &constraints.valid_pairings.uncommon,
            vec![
                &constraints.valid_pairings.common,
                &constraints.valid_pairings.rare,
            ],
        )
    } else {
        (
            &constraints.valid_pairings.rare,
            vec![
                &constraints.valid_pairings.common,
                &constraints.valid_pairings.uncommon,
            ],
        )
    };

    // Pick from the selected weight tier; fall back if it's empty
    let chosen_pool = if !pool.is_empty() {
        pool
    } else {
        fallbacks
            .into_iter()
            .find(|p| !p.is_empty())
            .unwrap_or_else(|| {
                eprintln!("No valid pairings found in archetype_constraints");
                std::process::exit(1);
            })
    };

    let pair = &chosen_pool[rng.random_range(0..chosen_pool.len())];
    (pair[0].clone(), pair[1].clone())
}

/// Resolve three-axis archetype selection through the constraint/funnel pipeline.
/// Returns (jungian_id, rpg_role_id, npc_role_id, resolved_name, resolution_source).
fn resolve_axes(
    pack: &GenrePack,
    cli: &Cli,
    rng: &mut impl Rng,
) -> (String, String, Option<String>, String, String) {
    let base = match &pack.base_archetypes {
        Some(b) => b,
        None => {
            // No base archetypes — fall back to legacy archetype name
            return legacy_axis_fallback(pack, cli, rng);
        }
    };
    let constraints = match &pack.archetype_constraints {
        Some(c) => c,
        None => {
            return legacy_axis_fallback(pack, cli, rng);
        }
    };

    // Determine jungian + rpg_role
    let (jungian_id, rpg_role_id) = match (&cli.jungian, &cli.rpg_role) {
        (Some(j), Some(r)) => (j.clone(), r.clone()),
        (Some(j), None) => {
            // Jungian specified, pick a valid rpg_role from pairings that include it
            let candidates: Vec<&[String; 2]> = constraints
                .valid_pairings
                .common
                .iter()
                .chain(&constraints.valid_pairings.uncommon)
                .chain(&constraints.valid_pairings.rare)
                .filter(|p| p[0] == *j)
                .collect();
            if candidates.is_empty() {
                eprintln!("No valid RPG role pairings found for jungian '{j}'");
                std::process::exit(1);
            }
            let pair = candidates[rng.random_range(0..candidates.len())];
            (j.clone(), pair[1].clone())
        }
        (None, Some(r)) => {
            // RPG role specified, pick a valid jungian from pairings that include it
            let candidates: Vec<&[String; 2]> = constraints
                .valid_pairings
                .common
                .iter()
                .chain(&constraints.valid_pairings.uncommon)
                .chain(&constraints.valid_pairings.rare)
                .filter(|p| p[1] == *r)
                .collect();
            if candidates.is_empty() {
                eprintln!("No valid Jungian pairings found for rpg_role '{r}'");
                std::process::exit(1);
            }
            let pair = candidates[rng.random_range(0..candidates.len())];
            (pair[0].clone(), r.clone())
        }
        (None, None) => select_weighted_pairing(constraints, rng),
    };

    // Determine NPC role
    let npc_role_id = cli.npc_role.clone().or_else(|| {
        if constraints.npc_roles_available.is_empty() {
            None
        } else {
            let idx = rng.random_range(0..constraints.npc_roles_available.len());
            Some(constraints.npc_roles_available[idx].clone())
        }
    });

    // Look up world funnels if --world was provided
    let funnels: Option<&ArchetypeFunnels> = cli.world.as_ref().and_then(|world_slug| {
        pack.worlds
            .get(world_slug)
            .and_then(|w| w.archetype_funnels.as_ref())
    });

    // Resolve through the Layered framework shim.
    match resolve_archetype(
        &jungian_id,
        &rpg_role_id,
        base,
        constraints,
        funnels,
        &cli.genre,
        cli.world.as_deref(),
    ) {
        Ok(result) => {
            // `ResolutionSource` is `#[non_exhaustive]`; the wildcard arm catches
            // any future variant and falls back to a Debug-formatted label so the
            // downstream consumer still gets a readable source string.
            let source = match result.source {
                ResolutionSource::WorldFunnel => "world_funnel".to_string(),
                ResolutionSource::GenreFallback => "genre_fallback".to_string(),
                other => format!("{other:?}").to_lowercase(),
            };
            (
                jungian_id,
                rpg_role_id,
                npc_role_id,
                result.resolved.name,
                source,
            )
        }
        Err(e) => {
            eprintln!("Archetype resolution failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Legacy fallback when no base_archetypes or archetype_constraints exist.
/// Populates axis fields from the old-style archetype selection.
fn legacy_axis_fallback(
    pack: &GenrePack,
    cli: &Cli,
    rng: &mut impl Rng,
) -> (String, String, Option<String>, String, String) {
    let archetype = if let Some(ref name) = cli.archetype {
        pack.archetypes
            .iter()
            .find(|a| a.name.as_str().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                eprintln!(
                    "Archetype '{}' not found. Available: {}",
                    name,
                    pack.archetypes
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                std::process::exit(1);
            })
    } else {
        &pack.archetypes[rng.random_range(0..pack.archetypes.len())]
    };

    let archetype_name = archetype.name.as_str().to_string();
    (
        cli.jungian.clone().unwrap_or_default(),
        cli.rpg_role.clone().unwrap_or_default(),
        cli.npc_role.clone(),
        archetype_name.clone(),
        "genre_fallback".to_string(),
    )
}

/// Weighted random selection from a trait pool.
/// Traits with `jungian_affinity` matching the NPC's jungian_id get 3x weight.
fn select_quirk(
    traits: &[NpcTrait],
    jungian_id: Option<&str>,
    rng: &mut impl Rng,
) -> Option<String> {
    if traits.is_empty() {
        return None;
    }
    let weights: Vec<f64> = traits
        .iter()
        .map(|t| {
            if let Some(jungian) = jungian_id {
                if t.jungian_affinity.iter().any(|a| a == jungian) {
                    3.0
                } else {
                    1.0
                }
            } else {
                1.0
            }
        })
        .collect();

    let total: f64 = weights.iter().sum();
    let mut roll = rng.random::<f64>() * total;
    for (i, w) in weights.iter().enumerate() {
        roll -= w;
        if roll <= 0.0 {
            return Some(traits[i].trait_name.clone());
        }
    }
    Some(traits.last()?.trait_name.clone())
}

fn generate_npc(
    pack: &GenrePack,
    genre_dir: &std::path::Path,
    cli: &Cli,
    rng: &mut impl Rng,
) -> NpcBlock {
    let corpus_dir = genre_dir.join("corpus");

    // Select culture
    let culture = if let Some(ref name) = cli.culture {
        pack.cultures
            .iter()
            .find(|c| c.name.as_str().eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                eprintln!(
                    "Culture '{}' not found. Available: {}",
                    name,
                    pack.cultures
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                std::process::exit(1);
            })
    } else {
        &pack.cultures[rng.random_range(0..pack.cultures.len())]
    };

    // Resolve three-axis archetype
    let (jungian_id, rpg_role_id, npc_role_id, resolved_archetype, resolution_source) =
        resolve_axes(pack, cli, rng);

    // Find the genre-pack NpcArchetype for personality/inventory/etc.
    // If we resolved through the new pipeline, try matching by resolved name first,
    // then fall back to --archetype flag, then random.
    let archetype = pack
        .archetypes
        .iter()
        .find(|a| a.name.as_str().eq_ignore_ascii_case(&resolved_archetype))
        .or_else(|| {
            cli.archetype.as_ref().and_then(|name| {
                pack.archetypes
                    .iter()
                    .find(|a| a.name.as_str().eq_ignore_ascii_case(name))
            })
        })
        .unwrap_or_else(|| &pack.archetypes[rng.random_range(0..pack.archetypes.len())]);

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
    let gender = cli
        .gender
        .clone()
        .unwrap_or_else(|| ["male", "female", "nonbinary"][rng.random_range(0..3)].to_string());
    let pronouns = match gender.as_str() {
        "male" => "he/him",
        "female" => "she/her",
        _ => "they/them",
    }
    .to_string();

    // OCEAN personality — prefer Jungian axis OCEAN tendencies when available
    let ocean = if !jungian_id.is_empty() {
        jitter_ocean_from_axes(&jungian_id, pack, archetype, rng)
    } else {
        jitter_ocean(archetype, rng)
    };
    let ocean_summary = summarize_ocean(&ocean);

    // Role
    let role = cli
        .role
        .clone()
        .unwrap_or_else(|| archetype.name.as_str().to_lowercase());

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

    // Select a random subset of dialogue quirks (3) so each NPC gets a unique voice
    let dialogue_quirks = select_quirk_subset(&archetype.dialogue_quirks, 3, rng);

    // Select spawn-tier quirks from the NPC traits database (1-2 quirks).
    let spawn_quirks = if let Some(ref db) = pack.npc_traits {
        let jungian_ref = if jungian_id.is_empty() {
            None
        } else {
            Some(jungian_id.as_str())
        };
        let mut quirks = Vec::new();
        // 1 personality quirk (weighted by jungian affinity)
        if let Some(q) = select_quirk(&db.personality, jungian_ref, rng) {
            quirks.push(q);
        }
        // 1 physical or behavioral quirk (50/50, no affinity weighting)
        let use_physical: bool = rng.random();
        let pool = if use_physical {
            &db.physical
        } else {
            &db.behavioral
        };
        if let Some(q) = select_quirk(pool, None, rng) {
            quirks.push(q);
        }
        quirks
    } else {
        vec![]
    };

    NpcBlock {
        name,
        pronouns,
        gender,
        culture: culture.name.as_str().to_string(),
        faction: culture.name.as_str().to_string(),
        faction_description: culture.description.clone(),
        archetype: resolved_archetype.clone(),
        role,
        appearance,
        personality: archetype.personality_traits.clone(),
        dialogue_quirks,
        history,
        ocean,
        ocean_summary,
        disposition: archetype.disposition_default,
        inventory: archetype.inventory_hints.clone(),
        stat_ranges: archetype.stat_ranges.clone(),
        trope_connections,
        jungian_id,
        rpg_role_id,
        npc_role_id,
        resolved_archetype,
        resolution_source,
        spawn_quirks,
    }
}

/// Select a random subset of `count` dialogue quirks from the pool.
/// If the pool has fewer than `count` entries, returns all of them (shuffled).
fn select_quirk_subset(quirks: &[String], count: usize, rng: &mut impl Rng) -> Vec<String> {
    use rand::seq::SliceRandom;
    let mut pool = quirks.to_vec();
    pool.shuffle(rng);
    pool.truncate(count);
    pool
}

/// OCEAN jitter using the Jungian archetype's OCEAN tendencies (range-based)
/// instead of the old archetype's single-point baseline.
fn jitter_ocean_from_axes(
    jungian_id: &str,
    pack: &GenrePack,
    fallback_archetype: &NpcArchetype,
    rng: &mut impl Rng,
) -> OceanValues {
    let tendencies = pack
        .base_archetypes
        .as_ref()
        .and_then(|base| base.jungian.iter().find(|j| j.id == jungian_id))
        .map(|j| &j.ocean_tendencies);

    match tendencies {
        Some(t) => {
            // Sample uniformly within each range, then apply small jitter
            let sample = |range: [f64; 2], rng: &mut dyn rand::RngCore| -> f64 {
                let base: f64 = rng.random_range(range[0]..=range[1]);
                let jitter: f64 = rng.random_range(-0.5..0.5);
                ((base + jitter).clamp(0.0, 10.0) * 10.0).round() / 10.0
            };
            OceanValues {
                openness: sample(t.openness, rng),
                conscientiousness: sample(t.conscientiousness, rng),
                extraversion: sample(t.extraversion, rng),
                agreeableness: sample(t.agreeableness, rng),
                neuroticism: sample(t.neuroticism, rng),
            }
        }
        None => jitter_ocean(fallback_archetype, rng),
    }
}

fn jitter_ocean(archetype: &NpcArchetype, rng: &mut impl Rng) -> OceanValues {
    let base = archetype
        .ocean
        .as_ref()
        .map(|o| {
            (
                o.openness,
                o.conscientiousness,
                o.extraversion,
                o.agreeableness,
                o.neuroticism,
            )
        })
        .unwrap_or((5.0, 5.0, 5.0, 5.0, 5.0));

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
        if v < 4.0 {
            low.to_string()
        } else if v > 7.0 {
            high.to_string()
        } else {
            mid.to_string()
        }
    }

    [
        label(
            o.openness,
            "conventional and practical",
            "balanced between tradition and novelty",
            "curious and imaginative",
        ),
        label(
            o.conscientiousness,
            "spontaneous and flexible",
            "moderately organized",
            "meticulous and disciplined",
        ),
        label(
            o.extraversion,
            "reserved and quiet",
            "selectively social",
            "outgoing and talkative",
        ),
        label(
            o.agreeableness,
            "blunt and competitive",
            "pragmatic",
            "warm and cooperative",
        ),
        label(
            o.neuroticism,
            "emotionally steady",
            "occasionally anxious",
            "easily stressed and reactive",
        ),
    ]
    .join(", ")
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
    "a bad trade went wrong",
    "their settlement was raided",
    "a mutation changed everything",
    "a drought drove them out",
    "they found something in the ruins they won't talk about",
    "a feud with another faction",
    "the water turned bad",
    "they lost someone important",
    "an Ancient device activated nearby",
    "a pack of beasts destroyed their homestead",
];

const HISTORY_DEEDS: &[&str] = &[
    "hard work and silence",
    "a timely warning about raiders",
    "fixing something nobody else could",
    "sharing water during the drought",
    "standing their ground when it mattered",
    "knowing where the good salvage was",
    "patching up the wounded after the last raid",
];

fn generate_history(
    faction: &str,
    role: &str,
    archetype: &NpcArchetype,
    rng: &mut impl Rng,
) -> String {
    let template = HISTORY_TEMPLATES[rng.random_range(0..HISTORY_TEMPLATES.len())];
    let event = HISTORY_EVENTS[rng.random_range(0..HISTORY_EVENTS.len())];
    let deed = HISTORY_DEEDS[rng.random_range(0..HISTORY_DEEDS.len())];
    let alt_role = archetype
        .typical_classes
        .first()
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "drifter".to_string());

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
