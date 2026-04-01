//! SideQuest Server — axum HTTP/WebSocket server.
//!
//! This is the main entry point for the SideQuest game server, providing HTTP and WebSocket
//! endpoints for the React frontend to interact with the game engine.

use std::collections::HashMap;

use clap::Parser;
use sidequest_agents::orchestrator::{Orchestrator, ScriptToolConfig};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use sidequest_server::{create_server, AppState, Args, Severity, WatcherEvent, WatcherEventType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    sidequest_server::init_tracing(args.trace());
    tracing::info!(
        port = args.port(),
        genre_packs = %args.genre_packs_path().display(),
        no_tts = args.no_tts(),
        headless = args.headless(),
        "SideQuest Server starting"
    );

    let (watcher_tx, watcher_rx) =
        tokio::sync::mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);

    let mut orchestrator = Orchestrator::new(watcher_tx);

    // Discover script tool binaries next to the server binary (ADR-056).
    // In dev: target/debug/sidequest-encountergen alongside target/debug/sidequest-server.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let genre_packs_path = args.genre_packs_path().to_string_lossy().to_string();

            // Encounter generator
            let encountergen_path = dir.join("sidequest-encountergen");
            if encountergen_path.exists() {
                tracing::info!(
                    path = %encountergen_path.display(),
                    "sidequest-encountergen discovered — narrator will have encounter tool"
                );
                orchestrator.register_script_tool("encountergen", ScriptToolConfig {
                    binary_path: encountergen_path.to_string_lossy().to_string(),
                    genre_packs_path: genre_packs_path.clone(),
                });
            } else {
                tracing::warn!(
                    expected = %encountergen_path.display(),
                    "sidequest-encountergen not found — narrator will not have encounter tool"
                );
            }

            // Starting loadout generator
            let loadoutgen_path = dir.join("sidequest-loadoutgen");
            if loadoutgen_path.exists() {
                tracing::info!(
                    path = %loadoutgen_path.display(),
                    "sidequest-loadoutgen discovered — narrator will have loadout tool"
                );
                orchestrator.register_script_tool("loadoutgen", ScriptToolConfig {
                    binary_path: loadoutgen_path.to_string_lossy().to_string(),
                    genre_packs_path: genre_packs_path.clone(),
                });
            }

            // NPC name generator (when merged from OQ-1)
            let namegen_path = dir.join("sidequest-namegen");
            if namegen_path.exists() {
                tracing::info!(
                    path = %namegen_path.display(),
                    "sidequest-namegen discovered — narrator will have NPC tool"
                );
                orchestrator.register_script_tool("namegen", ScriptToolConfig {
                    binary_path: namegen_path.to_string_lossy().to_string(),
                    genre_packs_path,
                });
            }
        }
    }

    let save_dir = args
        .save_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".sidequest")
                .join("saves")
        });

    let state = AppState::new_with_options(
        Box::new(orchestrator),
        args.genre_packs_path().to_path_buf(),
        save_dir,
        args.headless(),
    )
    .with_tts_disabled(args.no_tts() || args.headless());

    // Spawn the turn record bridge — receives TurnRecords from the orchestrator (hot path)
    // and broadcasts them as WatcherEvents to the GM dashboard (cold path).
    let bridge_state = state.clone();
    tokio::spawn(async move {
        turn_record_bridge(watcher_rx, bridge_state).await;
    });

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Shutdown on ctrl-c
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("SIGTERM received, initiating graceful shutdown");
        shutdown_tx.send(()).ok();
    });

    create_server(state, args.port(), shutdown_rx).await
}

/// Bridge TurnRecords from the orchestrator's mpsc channel into WatcherEvents
/// on the broadcast channel. This is the single point where per-turn telemetry
/// becomes visible to the GM dashboard.
async fn turn_record_bridge(
    mut rx: tokio::sync::mpsc::Receiver<TurnRecord>,
    state: AppState,
) {
    tracing::info!("turn record bridge started, awaiting TurnRecords");

    while let Some(record) = rx.recv().await {
        tracing::info!(
            turn_id = record.turn_id,
            intent = %record.classified_intent,
            agent = %record.agent_name,
            patches = record.patches_applied.len(),
            delta_empty = record.delta.is_empty(),
            extraction_tier = record.extraction_tier,
            is_degraded = record.is_degraded,
            agent_duration_ms = record.agent_duration_ms,
            token_count_in = record.token_count_in,
            token_count_out = record.token_count_out,
            "TurnRecord → WatcherEvent bridge"
        );

        let mut fields = HashMap::new();
        fields.insert("turn_id".into(), serde_json::json!(record.turn_id));
        fields.insert("player_input".into(), serde_json::json!(record.player_input));
        fields.insert("classified_intent".into(), serde_json::json!(record.classified_intent.to_string()));
        fields.insert("agent_name".into(), serde_json::json!(record.agent_name));
        fields.insert("agent_duration_ms".into(), serde_json::json!(record.agent_duration_ms));
        fields.insert("token_count_in".into(), serde_json::json!(record.token_count_in));
        fields.insert("token_count_out".into(), serde_json::json!(record.token_count_out));
        fields.insert("extraction_tier".into(), serde_json::json!(record.extraction_tier));
        fields.insert("is_degraded".into(), serde_json::json!(record.is_degraded));
        fields.insert("narration_len".into(), serde_json::json!(record.narration.len()));
        fields.insert(
            "patches".into(),
            serde_json::json!(
                record.patches_applied.iter()
                    .map(|p| serde_json::json!({
                        "patch_type": p.patch_type,
                        "fields_changed": p.fields_changed,
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        fields.insert("delta_empty".into(), serde_json::json!(record.delta.is_empty()));
        fields.insert(
            "beats_fired".into(),
            serde_json::json!(
                record.beats_fired.iter()
                    .map(|(name, thresh)| serde_json::json!({"trope": name, "threshold": thresh}))
                    .collect::<Vec<_>>()
            ),
        );

        fields.insert(
            "spans".into(),
            serde_json::json!(
                record.spans.iter()
                    .map(|(name, start_ms, dur_ms)| serde_json::json!({
                        "name": name,
                        "start_ms": start_ms,
                        "duration_ms": dur_ms,
                    }))
                    .collect::<Vec<_>>()
            ),
        );

        let severity = if record.is_degraded {
            Severity::Warn
        } else {
            Severity::Info
        };

        state.send_watcher_event(WatcherEvent {
            timestamp: record.timestamp,
            component: "orchestrator".into(),
            event_type: WatcherEventType::TurnComplete,
            severity,
            fields,
        });
    }

    tracing::info!("turn record bridge shutting down (channel closed)");
}
