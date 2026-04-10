//! Starting loadout generator binary.
//!
//! Generates a complete starting equipment set from genre pack inventory.yaml:
//! resolved items from the catalog, starting gold, currency name, and a narrative
//! hook describing the loadout in-fiction.
//!
//! Called by the narrator agent during character creation / session start.
//!
//! Usage:
//!   sidequest-loadoutgen --genre-packs-path ./genre_packs --genre low_fantasy --class fighter
//!   sidequest-loadoutgen --genre-packs-path ./genre_packs --genre space_opera --class pilot

use std::path::PathBuf;

use clap::Parser;
use rand::Rng;
use serde::Serialize;
use sidequest_genre::load_genre_pack;

#[derive(Parser)]
#[command(
    name = "sidequest-loadoutgen",
    about = "Generate starting equipment set from genre pack data"
)]
struct Cli {
    /// Path to the genre_packs/ directory. Also reads SIDEQUEST_CONTENT_PATH env var.
    #[arg(long, env = "SIDEQUEST_CONTENT_PATH")]
    genre_packs_path: PathBuf,

    /// Genre slug (e.g., low_fantasy, space_opera). Also reads SIDEQUEST_GENRE env var.
    #[arg(long, env = "SIDEQUEST_GENRE")]
    genre: String,

    /// Character class or archetype name (e.g., fighter, pilot).
    /// Matched case-insensitively against starting_equipment keys.
    #[arg(long)]
    class: String,

    /// Power tier (1-4) for scaling the loadout. Tier 1 = starting gear.
    /// Higher tiers add better items from the catalog. Defaults to 1.
    #[arg(long, default_value = "1")]
    tier: u32,
}

// ── Output types ────────────────────────────────────────────

#[derive(Serialize)]
struct LoadoutBlock {
    /// Character class/archetype the loadout is for.
    class: String,
    /// Currency name (e.g., "gold", "credits").
    currency_name: String,
    /// Starting gold/credits amount.
    starting_gold: u32,
    /// Resolved equipment items with full catalog details.
    equipment: Vec<LoadoutItem>,
    /// One-sentence narrative hook for how to introduce the loadout in-fiction.
    narrative_hook: String,
    /// Total value of all equipment.
    total_value: u32,
}

#[derive(Serialize)]
struct LoadoutItem {
    id: String,
    name: String,
    description: String,
    category: String,
    value: u32,
    tags: Vec<String>,
    lore: String,
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

    let inventory = match &pack.inventory {
        Some(inv) => inv,
        None => {
            eprintln!(
                "Genre pack '{}' has no inventory.yaml — cannot generate loadout",
                cli.genre
            );
            std::process::exit(1);
        }
    };

    let mut rng = rand::rng();
    let loadout = generate_loadout(inventory, &cli, &mut rng);

    let json = serde_json::to_string_pretty(&loadout).unwrap();
    println!("{json}");

    // Write sidecar JSONL so the orchestrator can see items for inventory tracking.
    write_sidecar(&loadout);
}

/// Write item_acquire records to the sidecar JSONL file for the orchestrator.
fn write_sidecar(loadout: &LoadoutBlock) {
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
        for item in &loadout.equipment {
            let record = serde_json::json!({
                "tool": "item_acquire",
                "result": {
                    "item_ref": &item.id,
                    "name": &item.name,
                    "category": &item.category
                }
            });
            let _ = writeln!(f, "{}", serde_json::to_string(&record).unwrap());
        }
    }
}

// ── Generation ──────────────────────────────────────────────

fn generate_loadout(
    inventory: &sidequest_genre::InventoryConfig,
    cli: &Cli,
    rng: &mut impl Rng,
) -> LoadoutBlock {
    // Find starting equipment for this class (case-insensitive match)
    let equipment_ids = inventory
        .starting_equipment
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&cli.class))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    // Find starting gold for this class
    let starting_gold = inventory
        .starting_gold
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&cli.class))
        .map(|(_, v)| *v)
        .unwrap_or(0);

    // Resolve item IDs to full catalog entries
    let mut equipment: Vec<LoadoutItem> = equipment_ids
        .iter()
        .filter_map(|id| {
            inventory
                .item_catalog
                .iter()
                .find(|item| item.id == *id)
                .map(|item| LoadoutItem {
                    id: item.id.clone(),
                    name: item.name.clone(),
                    description: item.description.clone(),
                    category: item.category.clone(),
                    value: item.value,
                    tags: item.tags.clone(),
                    lore: item.lore.clone(),
                })
        })
        .collect();

    // For higher tiers, add bonus items from the catalog matching the class's tags
    if cli.tier > 1 {
        let existing_ids: Vec<String> = equipment.iter().map(|e| e.id.clone()).collect();
        let tier_bonus_items: Vec<&sidequest_genre::CatalogItem> = inventory
            .item_catalog
            .iter()
            .filter(|item| {
                !existing_ids.contains(&item.id)
                    && item.power_level <= cli.tier
                    && item.power_level > 1
            })
            .collect();

        // Pick 1-2 bonus items from higher tiers
        let bonus_count = (cli.tier - 1).min(2) as usize;
        let mut picked = 0;
        for item in &tier_bonus_items {
            if picked >= bonus_count {
                break;
            }
            if rng.random_range(0..10) < 6 {
                equipment.push(LoadoutItem {
                    id: item.id.clone(),
                    name: item.name.clone(),
                    description: item.description.clone(),
                    category: item.category.clone(),
                    value: item.value,
                    tags: item.tags.clone(),
                    lore: item.lore.clone(),
                });
                picked += 1;
            }
        }
    }

    let total_value: u32 = equipment.iter().map(|e| e.value).sum();

    let currency_name = inventory
        .currency
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "gold".to_string());

    // Generate narrative hook
    let narrative_hook =
        build_narrative_hook(&cli.class, &equipment, starting_gold, &currency_name);

    LoadoutBlock {
        class: cli.class.clone(),
        currency_name,
        starting_gold,
        equipment,
        narrative_hook,
        total_value,
    }
}

// ── Narrative Hook ──────────────────────────────────────────

fn build_narrative_hook(
    _class: &str,
    equipment: &[LoadoutItem],
    gold: u32,
    currency: &str,
) -> String {
    let weapon = equipment.iter().find(|e| e.category == "weapon");
    let armor = equipment.iter().find(|e| e.category == "armor");

    let weapon_phrase = weapon
        .map(|w| format!("a {}", w.name.to_lowercase()))
        .unwrap_or_else(|| "nothing but your wits".to_string());

    let armor_phrase = armor
        .map(|a| format!("{}", a.name.to_lowercase()))
        .unwrap_or_else(|| "the clothes on your back".to_string());

    let gold_phrase = if gold > 0 {
        format!("{} {} to your name", gold, currency.to_lowercase())
    } else {
        "not a coin to your name".to_string()
    };

    format!(
        "You carry {} and wear {}, with {}.",
        weapon_phrase, armor_phrase, gold_phrase
    )
}
