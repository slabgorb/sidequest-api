//! Narrator prompt preview — renders the fully composed prompt from real Rust types.
//!
//! Uses the actual `NarratorAgent`, `ContextBuilder`, `PromptRegistry`, and SOUL
//! parser from sidequest-agents. The prompt can never drift from what the server
//! assembles at runtime because it IS the same code path.
//!
//! Usage:
//!   sidequest-promptpreview                                    # labeled (zone annotations)
//!   sidequest-promptpreview --raw                              # plain text as Claude sees it
//!   sidequest-promptpreview --test                             # pipe to claude -p
//!   sidequest-promptpreview --seed --genre mutant_wasteland    # real NPCs/encounters
//!   sidequest-promptpreview --combat                           # include combat rules
//!   sidequest-promptpreview --chase                            # include chase rules
//!   sidequest-promptpreview --dialogue                         # include dialogue rules
//!   sidequest-promptpreview --verbosity concise                # verbosity setting
//!   sidequest-promptpreview --vocabulary epic                  # vocabulary setting
//!   sidequest-promptpreview --action "I swing my wrench"       # custom player action

use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use sidequest_agents::agent::Agent;
use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::prompt_framework::{
    parse_soul_md, AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

#[derive(Parser)]
#[command(
    name = "sidequest-promptpreview",
    about = "Preview the fully composed narrator prompt using real Rust types"
)]
struct Cli {
    /// Strip zone/section labels — show plain text as Claude sees it.
    #[arg(long)]
    raw: bool,

    /// Pipe the raw prompt to `claude -p` and show the narrator's response.
    #[arg(long)]
    test: bool,

    /// Generate real NPCs/encounters from tool binaries instead of static placeholders.
    #[arg(long)]
    seed: bool,

    /// Genre for --seed (default: mutant_wasteland).
    #[arg(long, default_value = "mutant_wasteland")]
    genre: String,

    /// Path to genre_packs/ for --seed (auto-detected if omitted).
    #[arg(long, env = "SIDEQUEST_CONTENT_PATH")]
    genre_packs_path: Option<PathBuf>,

    /// Include combat narration rules (ADR-067).
    #[arg(long)]
    combat: bool,

    /// Include chase narration rules (ADR-067).
    #[arg(long)]
    chase: bool,

    /// Include dialogue narration rules (ADR-067).
    #[arg(long)]
    dialogue: bool,

    /// Narrator verbosity: concise, standard, verbose.
    #[arg(long, default_value = "standard")]
    verbosity: String,

    /// Narrator vocabulary: accessible, literary, epic.
    #[arg(long, default_value = "literary")]
    vocabulary: String,

    /// Custom player action text (overrides default placeholder).
    #[arg(long)]
    action: Option<String>,

    /// Claude model for --test mode.
    #[arg(long, default_value = "sonnet")]
    model: String,
}

/// Static placeholder game state for preview mode.
const PLACEHOLDER_STATE: &str = "\
<game_state>
Genre: mutant_wasteland
World: flickering_reach
Current location: The Collapsed Overpass

Players:
  - Rix (HP 18/22, Level 3, XP 450)
    Class: Scavenger
    Pronouns: they/them
    Inventory: [Rusty Pipe Wrench, Flickering Lantern, 3x Rad-Away]
    Gold: 47 scrap
    Abilities: [Jury-Rig, Scav Sense, Rad Resistance]

Active Quests:
  - \"The Signal Source\" (from: Toggler) — find the origin of the radio signal
  - \"Parts Run\" (from: Mama Cog) — retrieve capacitors from the old factory

NPCs present:
  - Patchwork (merchant, friendly) — trades salvage at The Overpass
  - Skitter (scout, wary) — watching from the scaffolding

Turn: 14
</game_state>";

const PLACEHOLDER_TROPE_BEATS: &str = "\
## Trope Beat Directive
The following narrative beat has fired and MUST be woven into this turn's narration:

[BEAT: \"The Mysterious Signal\" — Escalation]
The radio signal intensifies. Static resolves into fragments of a voice — not human,
not machine, something between. The direction is now unmistakable: it's coming from
beneath the old factory. This should feel ominous but compelling — the player should
WANT to investigate despite the danger.";

const PLACEHOLDER_ACTIVE_TROPES: &str = "\
Active Narrative Arcs:
- The Mysterious Signal (45% progressed): A strange radio signal pulses from the wasteland depths.
  Next beat at 60%: The signal becomes a voice — broken, pleading, inhuman.
- Patchwork's Debt (20% progressed): The merchant owes dangerous people.
  Next beat at 35%: A collector arrives at the Overpass.";

const PLACEHOLDER_SFX: &str = "\
metal_clang, radio_static, wind_howl, footsteps_gravel, \
door_creak, explosion_distant, gunshot_echo, creature_growl";

const PLACEHOLDER_ENCOUNTERS: &str = "\
<available_encounters>
Pre-generated enemies for this area. When combat starts, use enemies from this list.
Use their EXACT names, stats, and abilities. Do NOT invent different enemies.
If no combat this turn, ignore this section.

- 2x Salt Burrower (tier 2, HP 14 each) — eyeless ambush predators, chitin mandibles, burrow underground
  Weakness: bright light, fire. Abilities: Burrow Ambush, Mandible Crush.
- 1x Rad-Crawler (tier 3, HP 22) — six-legged mutant, radioactive carapace, territorial
  Weakness: cold, sonic. Abilities: Radiation Pulse, Leg Sweep, Shell Charge.
</available_encounters>";

const DEFAULT_ACTION: &str = "Something lunges at me from the dark. I swing my wrench at it.";

fn parse_verbosity(s: &str) -> NarratorVerbosity {
    match s.to_lowercase().as_str() {
        "concise" => NarratorVerbosity::Concise,
        "verbose" => NarratorVerbosity::Verbose,
        _ => NarratorVerbosity::Standard,
    }
}

fn parse_vocabulary(s: &str) -> NarratorVocabulary {
    match s.to_lowercase().as_str() {
        "accessible" => NarratorVocabulary::Accessible,
        "epic" => NarratorVocabulary::Epic,
        _ => NarratorVocabulary::Literary,
    }
}

/// Seed NPC data from the namegen binary. Returns prompt text or None.
fn seed_npcs(genre_packs_path: &str, genre: &str) -> Option<String> {
    let exe = find_binary("sidequest-namegen")?;
    let mut lines = Vec::new();
    for _ in 0..3 {
        let output = Command::new(&exe)
            .args(["--genre-packs-path", genre_packs_path, "--genre", genre])
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
        let name = json["name"].as_str().unwrap_or("Unknown");
        let role = json["role"].as_str().unwrap_or("unknown");
        let culture = json["culture"].as_str().unwrap_or("unknown");
        let summary = json["ocean_summary"].as_str().unwrap_or("");
        let quirks: Vec<&str> = json["dialogue_quirks"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(2).collect())
            .unwrap_or_default();
        lines.push(format!(
            "  - {} ({}, {}) — {}; {}",
            name,
            role,
            culture,
            summary,
            quirks.join("; ")
        ));
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "NPCs nearby (not yet met by player):\n{}",
        lines.join("\n")
    ))
}

/// Seed encounter data from the encountergen binary. Returns prompt text or None.
fn seed_encounters(genre_packs_path: &str, genre: &str) -> Option<String> {
    let exe = find_binary("sidequest-encountergen")?;
    let output = Command::new(&exe)
        .args([
            "--genre-packs-path",
            genre_packs_path,
            "--genre",
            genre,
            "--count",
            "2",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let enemies = json["enemies"].as_array()?;
    let mut enemy_lines = Vec::new();
    for e in enemies {
        let name = e["name"].as_str().unwrap_or("Unknown");
        let class = e["class"].as_str().unwrap_or("unknown");
        let tier = e["tier_label"].as_str().unwrap_or("?");
        let hp = e["hp"].as_u64().unwrap_or(0);
        let role = e["role"].as_str().unwrap_or("enemy");
        let abilities: Vec<&str> = e["abilities"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).take(3).collect())
            .unwrap_or_default();
        let weaknesses: Vec<&str> = e["weaknesses"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).take(2).collect())
            .unwrap_or_default();
        enemy_lines.push(format!(
            "- {} ({}, tier {}, HP {}) — {}\n  Abilities: {}. Weakness: {}.",
            name,
            class,
            tier,
            hp,
            role,
            abilities.join(", "),
            weaknesses.join(", ")
        ));
    }
    if enemy_lines.is_empty() {
        return None;
    }
    Some(format!(
        "<available_encounters>\n\
         Pre-generated enemies for this area. When combat starts, use enemies from this list.\n\
         Use their EXACT names, stats, and abilities. Do NOT invent different enemies.\n\
         If no combat this turn, ignore this section.\n\n\
         {}\n\
         </available_encounters>",
        enemy_lines.join("\n")
    ))
}

/// Find a sibling binary in the workspace target directory.
fn find_binary(name: &str) -> Option<PathBuf> {
    // Try relative to current exe first, then common build paths
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // Fallback: search in target/debug and target/release
    for profile in &["debug", "release"] {
        let candidate = PathBuf::from(format!("target/{}/{}", profile, name));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn main() {
    let cli = Cli::parse();

    // Locate SOUL.md relative to this binary's workspace root
    let soul_path = find_soul_md();
    let soul_data = parse_soul_md(&soul_path);

    if soul_data.is_empty() {
        eprintln!("WARNING: SOUL.md not found or empty at {}", soul_path.display());
    }

    // --- Build the prompt using real Rust types ---
    let narrator = NarratorAgent::new();
    let mut builder = ContextBuilder::new();

    // Step 1: Narrator's own sections (identity, constraints, agency, consequences, output, style, referral)
    narrator.build_context(&mut builder);

    // Step 2: Conditional mode sections (ADR-067)
    if cli.combat {
        narrator.build_combat_context(&mut builder);
    }
    if cli.chase {
        narrator.build_chase_context(&mut builder);
    }
    if cli.dialogue {
        narrator.build_dialogue_context(&mut builder);
    }

    // Step 3: SOUL principles (Early/Soul zone — same as orchestrator injection)
    let soul_text = soul_data.as_prompt_text_for("narrator");
    if !soul_text.is_empty() {
        builder.add_section(PromptSection::new(
            "soul_principles",
            soul_text,
            AttentionZone::Early,
            SectionCategory::Soul,
        ));
    }

    // Step 4: Trope beat directives (Early/State)
    builder.add_section(PromptSection::new(
        "trope_beat_directives",
        PLACEHOLDER_TROPE_BEATS,
        AttentionZone::Early,
        SectionCategory::State,
    ));

    // Step 5: Available encounters (Early/State)
    let encounter_text = if cli.seed {
        let genre_packs = cli
            .genre_packs_path
            .clone()
            .unwrap_or_else(detect_genre_packs_path);
        let gp = genre_packs.to_string_lossy();
        eprintln!("Seeding encounters from {} ...", cli.genre);
        seed_encounters(&gp, &cli.genre).unwrap_or_else(|| {
            eprintln!("  Encounters: FAILED (using static)");
            PLACEHOLDER_ENCOUNTERS.to_string()
        })
    } else {
        PLACEHOLDER_ENCOUNTERS.to_string()
    };
    builder.add_section(PromptSection::new(
        "available_encounters",
        encounter_text,
        AttentionZone::Early,
        SectionCategory::State,
    ));

    // Step 6: Game state (Valley/State) — with optional seeded NPCs
    let mut state_text = PLACEHOLDER_STATE.to_string();
    if cli.seed {
        let genre_packs = cli
            .genre_packs_path
            .clone()
            .unwrap_or_else(detect_genre_packs_path);
        let gp = genre_packs.to_string_lossy();
        eprintln!("Seeding NPCs from {} ...", cli.genre);
        if let Some(npc_text) = seed_npcs(&gp, &cli.genre) {
            eprintln!("  NPCs: seeded");
            // Insert NPC block before the closing </game_state> tag
            state_text = state_text.replace(
                "\nTurn: 14\n</game_state>",
                &format!("\n{}\n\nTurn: 14\n</game_state>", npc_text),
            );
        } else {
            eprintln!("  NPCs: FAILED (using static)");
        }
    }
    builder.add_section(PromptSection::new(
        "game_state",
        state_text,
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    // Step 7: Active tropes (Valley/State)
    builder.add_section(PromptSection::new(
        "active_tropes",
        PLACEHOLDER_ACTIVE_TROPES,
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    // Step 8: SFX library (Valley/State)
    builder.add_section(PromptSection::new(
        "sfx_library",
        format!(
            "[AVAILABLE SFX]\n\
             When your narration describes a sound-producing action, include matching \
             SFX IDs in sfx_triggers. Pick based on what HAPPENED, not what was mentioned.\n\
             Available: {}",
            PLACEHOLDER_SFX
        ),
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    // Step 9: Verbosity and vocabulary (Late/Format and Recency/Guardrail)
    // Use PromptRegistry to get the exact same text the server injects
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", parse_verbosity(&cli.verbosity));
    registry.register_vocabulary_section("narrator", parse_vocabulary(&cli.vocabulary));

    // Transfer registry sections into the builder
    for section in registry.registry("narrator") {
        builder.add_section(section.clone());
    }

    // Step 10: Player action (Recency/Action)
    let action_text = cli
        .action
        .as_deref()
        .unwrap_or(DEFAULT_ACTION);
    builder.add_section(PromptSection::new(
        "player_action",
        format!("The player says: {}", action_text),
        AttentionZone::Recency,
        SectionCategory::Action,
    ));

    // --- Render output ---
    if cli.test {
        let prompt = builder.compose();
        let word_count: usize = prompt.split_whitespace().count();
        eprintln!(
            "Testing prompt (~{} words) against claude -p --model {} ...",
            word_count, cli.model
        );
        let result = Command::new("claude")
            .args(["-p", "--model", &cli.model, &prompt])
            .output();
        match result {
            Ok(output) if output.status.success() => {
                print!("{}", String::from_utf8_lossy(&output.stdout));
            }
            Ok(output) => {
                eprintln!(
                    "ERROR: claude -p failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("ERROR: failed to run claude: {}", e);
                std::process::exit(1);
            }
        }
    } else if cli.raw {
        println!("{}", builder.compose());
    } else {
        // Labeled output with zone annotations
        let breakdown = builder.zone_breakdown();
        let total_tokens: usize = breakdown
            .zones
            .iter()
            .map(|z| z.total_tokens)
            .sum();
        let section_count: usize = breakdown
            .zones
            .iter()
            .map(|z| z.sections.len())
            .sum();

        println!(
            "Narrator Prompt Preview  ({} sections, ~{} tokens)",
            section_count, total_tokens
        );
        println!(
            "SOUL.md: {} ({} principles)",
            soul_path.display(),
            soul_data.len()
        );
        println!();

        let sorted_sections = builder.build();
        let mut current_zone: Option<AttentionZone> = None;

        for section in &sorted_sections {
            if Some(section.zone) != current_zone {
                if current_zone.is_some() {
                    println!();
                }
                println!("{}", "=".repeat(72));
                println!(
                    "--- ZONE: {:?} (priority {}) ---",
                    section.zone,
                    section.zone.order()
                );
                println!("{}", "=".repeat(72));
                println!();
                current_zone = Some(section.zone);
            }

            let token_est = section.token_estimate();
            println!(
                "[section: {}]  (category: {:?}, ~{} tokens)",
                section.name, section.category, token_est
            );
            println!("{}", "-".repeat(60));
            println!("{}", section.content);
            println!();
        }
    }
}

/// Locate SOUL.md — check workspace root, then relative to exe.
fn find_soul_md() -> PathBuf {
    // Check relative to CWD (common when running from workspace root)
    let cwd_path = PathBuf::from("SOUL.md");
    if cwd_path.exists() {
        return cwd_path;
    }

    // Check relative to exe
    if let Ok(exe) = std::env::current_exe() {
        // exe is in target/debug/ or target/release/, workspace is ../../
        if let Some(workspace) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            let candidate = workspace.join("SOUL.md");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // Fallback — will produce a warning when parsed
    PathBuf::from("SOUL.md")
}

/// Auto-detect genre_packs path relative to the workspace.
fn detect_genre_packs_path() -> PathBuf {
    // Check sibling sidequest-content repo (orchestrator layout)
    let orchestrator_path = PathBuf::from("../sidequest-content/genre_packs");
    if orchestrator_path.exists() {
        return orchestrator_path;
    }

    // Check relative to exe
    if let Ok(exe) = std::env::current_exe() {
        if let Some(workspace) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            let candidate = workspace
                .parent()
                .map(|p| p.join("sidequest-content/genre_packs"))
                .unwrap_or_default();
            if candidate.exists() {
                return candidate;
            }
        }
    }

    PathBuf::from("../sidequest-content/genre_packs")
}
