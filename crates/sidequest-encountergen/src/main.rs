//! Encounter generator binary.
//!
//! Generates enemy stat blocks from genre pack data: culture-appropriate name
//! (Markov chains), class/archetype stats, HP, abilities, weaknesses,
//! disposition, OCEAN jitter, trope connections, and a visual prompt for
//! the daemon renderer.
//!
//! Called by the narrator/creature_smith agent when introducing enemies.
//!
//! Usage:
//!   sidequest-encountergen --genre-packs-path ./genre_packs --genre mutant_wasteland
//!   sidequest-encountergen --genre-packs-path ./genre_packs --genre mutant_wasteland --tier 2 --count 3
//!   sidequest-encountergen --genre-packs-path ./genre_packs --genre mutant_wasteland --role "ambush predator" --class Beastkin

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use rand::Rng;
use serde::Serialize;
use sidequest_genre::{load_genre_pack, GenrePack, NpcArchetype};

#[derive(Parser)]
#[command(
    name = "sidequest-encountergen",
    about = "Generate enemy encounter stat blocks from genre pack data"
)]
struct Cli {
    /// Path to the genre_packs/ directory. Also reads SIDEQUEST_CONTENT_PATH env var.
    #[arg(long, env = "SIDEQUEST_CONTENT_PATH")]
    genre_packs_path: PathBuf,

    /// Genre slug (e.g., mutant_wasteland). Also reads SIDEQUEST_GENRE env var.
    #[arg(long, env = "SIDEQUEST_GENRE")]
    genre: String,

    /// World slug (e.g., grimvault). When set, checks for worlds/{world}/creatures.yaml
    /// and samples from creature definitions instead of generating humanoid NPCs.
    #[arg(long)]
    world: Option<String>,

    /// Power tier (1-4, maps to level ranges). Random if omitted.
    #[arg(long)]
    tier: Option<u32>,

    /// Number of enemies to generate. Defaults to 1.
    #[arg(long, default_value = "1")]
    count: u32,

    /// Enemy role hint (e.g., "ambush predator", "pack scout"). Flavors the stat block.
    #[arg(long)]
    role: Option<String>,

    /// Character class (e.g., Scavenger, Mutant). Random from allowed_classes if omitted.
    #[arg(long)]
    class: Option<String>,

    /// Culture name for name generation. Random if omitted.
    #[arg(long)]
    culture: Option<String>,

    /// Archetype name (e.g., "Wasteland Trader"). Random if omitted.
    #[arg(long)]
    archetype: Option<String>,

    /// Context hint for the encounter (e.g., "guarding a bridge", "ambush in ruins").
    /// Used to flavor the visual prompt.
    #[arg(long)]
    context: Option<String>,
}

// ── Output types ────────────────────────────────────────────

#[derive(Serialize)]
struct EncounterBlock {
    enemies: Vec<EnemyBlock>,
}

#[derive(Serialize)]
struct EnemyBlock {
    name: String,
    class: String,
    race: String,
    level: u32,
    tier_label: String,
    role: String,
    hp: u32,
    abilities: Vec<String>,
    weaknesses: Vec<String>,
    disposition: i32,
    personality: Vec<String>,
    dialogue_quirks: Vec<String>,
    inventory: Vec<String>,
    stat_scores: HashMap<String, i32>,
    ocean: OceanValues,
    ocean_summary: String,
    trope_connections: Vec<TropeConnection>,
    visual_prompt: String,
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

// ── Tier mapping ────────────────────────────────────────────

/// Map tier number (1-4) to a level range.
fn tier_to_level_range(tier: u32) -> (u32, u32) {
    match tier {
        1 => (1, 3),
        2 => (4, 6),
        3 => (7, 9),
        4 => (10, 10),
        _ => (1, 3),
    }
}

/// Find the power tier entry for a given class and level.
fn find_power_tier<'a>(
    power_tiers: &'a HashMap<String, Vec<sidequest_genre::PowerTier>>,
    class: &str,
    level: u32,
) -> Option<&'a sidequest_genre::PowerTier> {
    power_tiers.get(class).and_then(|tiers| {
        tiers
            .iter()
            .find(|t| level >= t.level_range[0] && level <= t.level_range[1])
    })
}

// ── Main ────────────────────────────────────────────────────

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
    let mut enemies = Vec::with_capacity(cli.count as usize);

    // Check for world-level creatures.yaml — if present, sample from creature
    // definitions instead of generating humanoid NPCs from rules.yaml.
    let creatures_path = cli
        .world
        .as_ref()
        .map(|w| genre_dir.join("worlds").join(w).join("creatures.yaml"));
    let creatures: Option<Vec<serde_yaml::Value>> = creatures_path
        .as_ref()
        .filter(|p| p.exists())
        .and_then(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            let doc: serde_yaml::Value = serde_yaml::from_str(&text).ok()?;
            doc.get("creatures")
                .and_then(|c| c.as_sequence())
                .map(|seq| seq.to_vec())
        });

    if let Some(ref creature_list) = creatures {
        if !creature_list.is_empty() {
            // Filter by tier if specified
            let filtered: Vec<&serde_yaml::Value> = if let Some(tier) = cli.tier {
                creature_list
                    .iter()
                    .filter(|c| {
                        c.get("threat_level")
                            .and_then(|t| t.as_u64())
                            .map(|t| t == tier as u64)
                            .unwrap_or(false)
                    })
                    .collect()
            } else {
                creature_list.iter().collect()
            };

            let pool = if filtered.is_empty() {
                creature_list.iter().collect::<Vec<_>>()
            } else {
                filtered
            };

            for _ in 0..cli.count {
                let creature = pool[rng.random_range(0..pool.len())];
                enemies.push(creature_to_enemy_block(creature, &mut rng));
            }

            let block = EncounterBlock { enemies };
            let json = serde_json::to_string_pretty(&block).unwrap();
            println!("{json}");
            write_sidecar(&block);
            return;
        }
    }

    // Fallback: generate humanoid NPCs from rules.yaml (original path)
    for _ in 0..cli.count {
        enemies.push(generate_enemy(&pack, &genre_dir, &cli, &mut rng));
    }

    let block = EncounterBlock { enemies };
    let json = serde_json::to_string_pretty(&block).unwrap();
    println!("{json}");

    // Write sidecar JSONL so the orchestrator can see this tool was called.
    write_sidecar(&block);
}

/// Write tool call records to the sidecar JSONL file for the orchestrator.
fn write_sidecar(block: &EncounterBlock) {
    let dir = match std::env::var("SIDEQUEST_TOOL_SIDECAR_DIR") {
        Ok(d) => d,
        Err(_) => return,
    };
    let session_id = match std::env::var("SIDEQUEST_TOOL_SESSION_ID") {
        Ok(s) => s,
        Err(_) => return,
    };

    let sidecar_path =
        std::path::PathBuf::from(&dir).join(format!("sidequest-tools-{session_id}.jsonl"));
    let _ = std::fs::create_dir_all(&dir);

    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&sidecar_path)
    {
        for enemy in &block.enemies {
            let record = serde_json::json!({
                "tool": "personality_event",
                "result": {
                    "npc": &enemy.name,
                    "event_type": "introduced",
                    "description": format!("enemy: {} (tier {})", &enemy.role, &enemy.tier_label)
                }
            });
            let _ = writeln!(f, "{}", serde_json::to_string(&record).unwrap());
        }
    }
}

// ── Creature YAML → EnemyBlock ─────────────────────────────

/// Convert a creature definition from creatures.yaml into an EnemyBlock.
fn creature_to_enemy_block(creature: &serde_yaml::Value, rng: &mut impl Rng) -> EnemyBlock {
    let str_field = |key: &str| -> String {
        creature
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let u64_field = |key: &str, default: u64| -> u64 {
        creature
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(default)
    };

    let name = str_field("name");
    let threat_level = u64_field("threat_level", 1) as u32;
    let hp = u64_field("hp", 4) as u32;
    let ac = u64_field("ac", 10);
    let damage = str_field("damage");
    let morale = str_field("morale");

    let abilities: Vec<String> = creature
        .get("abilities")
        .and_then(|a| a.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|a| {
                    let aname = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let adesc = a.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    if aname.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "{} — {}",
                            aname,
                            adesc.chars().take(80).collect::<String>()
                        ))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let tags: Vec<String> = creature
        .get("tags")
        .and_then(|t| t.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let tier_label = format!("tier-{}", threat_level);

    EnemyBlock {
        name,
        class: "creature".to_string(),
        race: tags.first().cloned().unwrap_or_else(|| "beast".to_string()),
        level: threat_level,
        tier_label,
        role: if !damage.is_empty() {
            format!("{}, morale: {}", damage, morale)
        } else {
            morale
        },
        hp,
        abilities,
        weaknesses: vec![format!("AC {}", ac)],
        disposition: -20, // creatures are hostile
        personality: vec![],
        dialogue_quirks: vec![],
        inventory: creature
            .get("loot")
            .and_then(|l| l.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| {
                        v.get("description")
                            .and_then(|d| d.as_str())
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        stat_scores: HashMap::new(),
        ocean: OceanValues {
            openness: rng.random_range(1.0..4.0),
            conscientiousness: rng.random_range(2.0..5.0),
            extraversion: rng.random_range(2.0..6.0),
            agreeableness: rng.random_range(1.0..3.0),
            neuroticism: rng.random_range(4.0..8.0),
        },
        ocean_summary: "feral and aggressive".to_string(),
        trope_connections: vec![],
        visual_prompt: creature
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(200)
            .collect(),
    }
}

// ── Generation ──────────────────────────────────────────────

fn generate_enemy(
    pack: &GenrePack,
    genre_dir: &std::path::Path,
    cli: &Cli,
    rng: &mut impl Rng,
) -> EnemyBlock {
    let corpus_dir = genre_dir.join("corpus");

    // Select class
    let class = if let Some(ref c) = cli.class {
        if !pack
            .rules
            .allowed_classes
            .iter()
            .any(|ac| ac.eq_ignore_ascii_case(c))
        {
            eprintln!(
                "Class '{}' not found. Available: {}",
                c,
                pack.rules.allowed_classes.join(", ")
            );
            std::process::exit(1);
        }
        c.clone()
    } else {
        pack.rules.allowed_classes[rng.random_range(0..pack.rules.allowed_classes.len())].clone()
    };

    // Select race
    let race =
        pack.rules.allowed_races[rng.random_range(0..pack.rules.allowed_races.len())].clone();

    // Select tier and level
    let tier = cli.tier.unwrap_or_else(|| rng.random_range(1..=3)); // tier 4 (level 10) is rare for NPCs
    let (level_min, level_max) = tier_to_level_range(tier);
    let level = rng.random_range(level_min..=level_max);

    // HP from class base * level (simple formula matching rules.yaml)
    let base_hp = pack.rules.class_hp_bases.get(&class).copied().unwrap_or(8);
    let hp = base_hp * level;

    // Select archetype
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

    // Select culture and generate name
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

    let generator = sidequest_genre::names::build_from_culture(culture, &corpus_dir, rng);
    let mut name = String::new();
    for _ in 0..10 {
        let candidate = generator.generator.generate_person(rng);
        if !candidate.is_empty()
            && !candidate.to_lowercase().starts_with("of ")
            && !candidate.to_lowercase().starts_with("the ")
        {
            name = candidate;
            break;
        }
    }
    if name.is_empty() {
        name = generator.generator.generate_person(rng);
    }

    // Role
    let role = cli
        .role
        .clone()
        .unwrap_or_else(|| archetype.name.as_str().to_lowercase());

    // Generate ability scores from archetype stat_ranges
    let stat_scores: HashMap<String, i32> = pack
        .rules
        .ability_score_names
        .iter()
        .map(|stat_name| {
            let value = archetype
                .stat_ranges
                .get(stat_name)
                .map(|range| rng.random_range(range[0]..=range[1]))
                .unwrap_or_else(|| rng.random_range(8..=14));
            (stat_name.clone(), value)
        })
        .collect();

    // OCEAN personality (jittered from archetype baseline)
    let ocean = jitter_ocean(archetype, rng);
    let ocean_summary = summarize_ocean(&ocean);

    // Abilities — derive from class + tier + archetype
    let abilities = generate_abilities(&class, tier, archetype, rng);

    // Weaknesses — derive from class + race
    let weaknesses = generate_weaknesses(&class, &race, rng);

    // Trope connections
    let trope_connections = match_tropes(&pack.tropes, archetype, culture);

    // Tier label + visual prompt (computed before moving `class` into the struct)
    let tier_label = find_power_tier(&pack.power_tiers, &class, level)
        .map(|t| t.label.clone())
        .unwrap_or_else(|| format!("tier-{}", tier));

    let visual_prompt = build_visual_prompt(pack, &class, level, archetype, cli.context.as_deref());

    EnemyBlock {
        name,
        class,
        race,
        level,
        tier_label,
        role,
        hp,
        abilities,
        weaknesses,
        disposition: archetype.disposition_default.min(-10), // enemies skew hostile
        personality: archetype.personality_traits.clone(),
        dialogue_quirks: archetype.dialogue_quirks.clone(),
        inventory: archetype.inventory_hints.clone(),
        stat_scores,
        ocean,
        ocean_summary,
        trope_connections,
        visual_prompt,
    }
}

// ── Abilities ───────────────────────────────────────────────

/// Generate abilities from class + tier. Higher tiers unlock more powerful abilities.
fn generate_abilities(
    class: &str,
    tier: u32,
    archetype: &NpcArchetype,
    rng: &mut impl Rng,
) -> Vec<String> {
    let mut abilities = Vec::new();

    // Base class abilities (every enemy gets at least one)
    let class_abilities: &[&[&str]] = match class.to_lowercase().as_str() {
        "scavenger" => &[
            &["Scrap Throw", "Quick Loot", "Improvised Trap"],
            &["Ambush", "Jury-Rig Weapon", "Escape Artist"],
            &["Salvage Mastery", "Trap Network", "Ghost Walk"],
            &["Vaultbreaker Strike", "Scrap Golem", "Perfect Ambush"],
        ],
        "mutant" => &[
            &["Toxic Spit", "Hardened Skin", "Feral Charge"],
            &["Acid Blood", "Regeneration", "Bioluminescent Flash"],
            &["Mutation Surge", "Toxic Cloud", "Adaptive Armor"],
            &["Apex Transformation", "Radioactive Aura", "Evolution Burst"],
        ],
        "pureblood" => &[
            &["First Aid", "Old-World Knowledge", "Steady Aim"],
            &["Field Surgery", "Tactical Analysis", "Precision Shot"],
            &[
                "Command Presence",
                "Pre-War Tech Override",
                "Suppressing Fire",
            ],
            &[
                "Architect's Will",
                "Orbital Strike Beacon",
                "Civilization's Shield",
            ],
        ],
        "synth" => &[
            &["Overclock", "Synthetic Resilience", "Scan"],
            &["Integrated Weapon", "Self-Repair", "EMP Pulse"],
            &["Combat Protocol", "System Override", "Drone Deploy"],
            &["Sovereign Mode", "Nanite Swarm", "Full System Integration"],
        ],
        "beastkin" => &[
            &["Feral Bite", "Pack Instinct", "Keen Senses"],
            &["Predator's Leap", "Territorial Roar", "Venom Strike"],
            &["Alpha Command", "Primal Fury", "Nature's Armor"],
            &["Apex Predator", "Pack Swarm", "Primal Lord's Presence"],
        ],
        "tinker" => &[
            &["Jury-Rig", "Shock Prod", "Smoke Bomb"],
            &["Turret Deploy", "Electrified Net", "Gadget Barrage"],
            &["Mech Suit Engage", "Tesla Coil", "Drone Swarm"],
            &[
                "Forge Master's Arsenal",
                "Fabricator Beam",
                "Walking Workshop",
            ],
        ],
        _ => &[
            &["Strike", "Defend", "Retreat"],
            &["Power Strike", "Taunt", "Rally"],
            &["Devastating Blow", "Battle Cry", "Last Stand"],
            &["Ultimate Strike", "Overwhelming Force", "Unstoppable"],
        ],
    };

    let tier_idx = (tier as usize)
        .saturating_sub(1)
        .min(class_abilities.len() - 1);

    // Pick abilities from current tier and below
    for t in 0..=tier_idx {
        let pool = class_abilities[t];
        // 1-2 abilities per tier, weighted toward current tier
        let pick_count = if t == tier_idx { 2 } else { 1 };
        let mut picked = 0;
        for ability in pool {
            if picked >= pick_count {
                break;
            }
            // 70% chance to include each ability
            if rng.random_range(0..10) < 7 {
                abilities.push(ability.to_string());
                picked += 1;
            }
        }
        // Guarantee at least one from current tier
        if t == tier_idx && picked == 0 {
            abilities.push(pool[rng.random_range(0..pool.len())].to_string());
        }
    }

    // Add one archetype-flavored ability if typical_classes overlap
    if archetype
        .typical_classes
        .iter()
        .any(|c| c.eq_ignore_ascii_case(class))
    {
        let archetype_flavor = format!("{}'s Instinct", archetype.name.as_str());
        abilities.push(archetype_flavor);
    }

    abilities
}

// ── Weaknesses ──────────────────────────────────────────────

fn generate_weaknesses(class: &str, race: &str, rng: &mut impl Rng) -> Vec<String> {
    let mut weaknesses = Vec::new();

    // Class-based weaknesses
    match class.to_lowercase().as_str() {
        "scavenger" => weaknesses.push("low durability — light or no armor".to_string()),
        "mutant" => weaknesses.push("radiation dependency — weakens in clean zones".to_string()),
        "pureblood" => {
            weaknesses.push("contamination vulnerability — no mutation resistance".to_string())
        }
        "synth" => {
            weaknesses.push("EMP vulnerability — stunned by electromagnetic pulses".to_string())
        }
        "beastkin" => weaknesses.push("fire aversion — panics near open flame".to_string()),
        "tinker" => {
            weaknesses.push("gadget fragility — abilities break on critical failure".to_string())
        }
        _ => weaknesses.push("no special resistances".to_string()),
    }

    // Race-based weakness (50% chance for a second weakness)
    if rng.random_range(0..2) == 0 {
        match race.to_lowercase().as_str() {
            r if r.contains("mutant") => {
                weaknesses.push("unstable mutations — random debuff under stress".to_string())
            }
            r if r.contains("synthetic") => {
                weaknesses.push("memory fragmentation — confused by paradoxes".to_string())
            }
            r if r.contains("plant") => {
                weaknesses.push("drought vulnerability — weakened without water".to_string())
            }
            r if r.contains("animal") || r.contains("uplifted") => {
                weaknesses.push("pack instinct — morale breaks when isolated".to_string())
            }
            _ => {}
        }
    }

    weaknesses
}

// ── OCEAN ───────────────────────────────────────────────────

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

// ── Tropes ──────────────────────────────────────────────────

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

// ── Visual Prompt ───────────────────────────────────────────

/// Build an image generation prompt from power_tiers NPC description + visual_style.
fn build_visual_prompt(
    pack: &GenrePack,
    class: &str,
    level: u32,
    archetype: &NpcArchetype,
    context: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    // NPC appearance from power tier (if available)
    if let Some(tier) = find_power_tier(&pack.power_tiers, class, level) {
        if let Some(ref npc_desc) = tier.npc {
            parts.push(npc_desc.clone());
        } else {
            // Highest tier — use player description as fallback
            parts.push(tier.player.clone());
        }
    } else {
        // No power tier — use archetype description
        parts.push(archetype.description.clone());
    }

    // Context hint (e.g., "guarding a bridge")
    if let Some(ctx) = context {
        parts.push(ctx.to_string());
    }

    // Genre visual style suffix
    parts.push(pack.visual_style.positive_suffix.clone());

    // Join with commas, clean up whitespace
    parts
        .iter()
        .map(|p| p.trim().trim_end_matches(','))
        .collect::<Vec<_>>()
        .join(", ")
}
