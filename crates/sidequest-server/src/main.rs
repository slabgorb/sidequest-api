//! SideQuest Server — axum HTTP/WebSocket server.
//!
//! This is the main entry point for the SideQuest game server, providing HTTP and WebSocket
//! endpoints for the React frontend to interact with the game engine.

use clap::Parser;
use sidequest_agents::orchestrator::Orchestrator;
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use sidequest_server::{create_server, AppState, Args, Severity, WatcherEventBuilder, WatcherEventType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    sidequest_server::init_tracing(false);
    tracing::info!(
        port = args.port(),
        genre_packs = %args.genre_packs_path().display(),
        no_tts = args.no_tts(),
        headless = args.headless(),
        "SideQuest Server starting"
    );

    let (watcher_tx, watcher_rx) =
        tokio::sync::mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);

    let orchestrator = Orchestrator::new_with_otel(watcher_tx, args.otel_endpoint().map(|s| s.to_string()));

    // ADR-059: Tool binaries are now called server-side by dispatch/pregen.rs,
    // not registered on the orchestrator for narrator tool calls.
    // Binary paths are discovered and stored on AppState (below).

    let save_dir = args
        .save_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".sidequest")
                .join("saves")
        });

    // Store discovered binary paths for server-side pre-generation (ADR-059)
    let (namegen_for_state, encountergen_for_state, loadoutgen_for_state) =
        if let Ok(exe) = std::env::current_exe() {
            let dir = exe.parent();
            (
                dir.map(|d| d.join("sidequest-namegen")).filter(|p| p.exists()),
                dir.map(|d| d.join("sidequest-encountergen")).filter(|p| p.exists()),
                dir.map(|d| d.join("sidequest-loadoutgen")).filter(|p| p.exists()),
            )
        } else {
            (None, None, None)
        };

    let mut state = AppState::new_with_game_service(
        Box::new(orchestrator),
        args.genre_packs_path().to_path_buf(),
        save_dir,
    )
    .with_tts_disabled(args.no_tts() || args.headless());

    if args.headless() {
        state = state.with_render_disabled();
    }

    if let Some(endpoint) = args.otel_endpoint() {
        state = state.with_otel_endpoint(endpoint.to_string());
    }

    if let Some(path) = namegen_for_state {
        state = state.with_namegen_binary(path);
    }
    if let Some(path) = encountergen_for_state {
        state = state.with_encountergen_binary(path);
    }
    if let Some(path) = loadoutgen_for_state {
        state = state.with_loadoutgen_binary(path);
    }

    // Spawn the turn record bridge — receives TurnRecords from the orchestrator (hot path)
    // and broadcasts them as WatcherEvents to the GM dashboard (cold path).
    tokio::spawn(async move {
        turn_record_bridge(watcher_rx).await;
    });

    // Spawn history capture — subscribes to the global telemetry channel and
    // stores events for replay to late-connecting GM panels.
    {
        let history_state = state.clone();
        let mut history_rx = state.subscribe_watcher();
        tokio::spawn(async move {
            while let Ok(event) = history_rx.recv().await {
                history_state.store_watcher_event(event);
            }
        });
    }

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
) {
    tracing::info!("turn record bridge started, awaiting TurnRecords");

    while let Some(record) = rx.recv().await {
        tracing::info!(
            turn_id = record.turn_id,
            intent = %record.classified_intent,
            agent = %record.agent_name,
            patches = record.patches_applied.len(),
            delta_empty = record.delta.is_empty(),
            is_degraded = record.is_degraded,
            agent_duration_ms = record.agent_duration_ms,
            token_count_in = record.token_count_in,
            token_count_out = record.token_count_out,
            "TurnRecord → WatcherEvent bridge"
        );

        let patches: Vec<serde_json::Value> = record.patches_applied.iter()
            .map(|p| serde_json::json!({
                "patch_type": p.patch_type,
                "fields_changed": p.fields_changed,
            }))
            .collect();
        let beats_fired: Vec<serde_json::Value> = record.beats_fired.iter()
            .map(|(name, thresh)| serde_json::json!({"trope": name, "threshold": thresh}))
            .collect();
        let spans: Vec<serde_json::Value> = record.spans.iter()
            .map(|(name, start_ms, dur_ms)| serde_json::json!({
                "name": name,
                "start_ms": start_ms,
                "duration_ms": dur_ms,
            }))
            .collect();

        let mut builder = WatcherEventBuilder::new("orchestrator", WatcherEventType::TurnComplete)
            .timestamp(record.timestamp)
            .field("turn_id", &record.turn_id)
            .field("player_input", &record.player_input)
            .field("classified_intent", record.classified_intent.to_string())
            .field("agent_name", &record.agent_name)
            .field("agent_duration_ms", record.agent_duration_ms)
            .field("token_count_in", record.token_count_in)
            .field("token_count_out", record.token_count_out)
            .field("is_degraded", record.is_degraded)
            .field("narration_len", record.narration.len())
            .field("patches", &patches)
            .field("delta_empty", record.delta.is_empty())
            .field("beats_fired", &beats_fired)
            .field("spans", &spans);
        if record.is_degraded {
            builder = builder.severity(Severity::Warn);
        }
        builder.send();
    }

    tracing::info!("turn record bridge shutting down (channel closed)");
}
