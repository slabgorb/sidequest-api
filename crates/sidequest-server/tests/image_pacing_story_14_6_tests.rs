//! Story 14-6: Image pacing throttle — configurable cooldown for image generation.
//!
//! RED phase — these tests reference types and logic that don't exist yet:
//!   - `ImagePacingConfig` on SessionEventPayload
//!   - `ImagePacingThrottle` in render_integration
//!   - Default cooldowns: 60s multiplayer, 30s solo
//!
//! ACs tested:
//!   AC1: `image_cooldown_seconds` field on SessionEventPayload (u32)
//!   AC2: Default cooldown: 60s multiplayer, 30s solo
//!   AC3: Throttle suppresses renders within cooldown window
//!   AC4: Cooldown resets after period expires
//!   AC5: DM force override bypasses throttle
//!   AC6: Wire format — field serializes/deserializes correctly

use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

use sidequest_game::render_queue::RenderJobResult;
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};
use sidequest_protocol::{GameMessage, SessionEventPayload};
use sidequest_server::render_integration::{
    ImagePacingThrottle, RenderResultContext,
};

// =========================================================================
// Helper: build test subjects and render contexts
// =========================================================================

fn test_subject() -> RenderSubject {
    RenderSubject::new(
        vec!["Kira".to_string()],
        SceneType::Exploration,
        SubjectTier::Scene,
        "A torchlit corridor stretches into darkness".to_string(),
        0.7,
    )
    .expect("valid test subject")
}

fn test_render_context(index: u32) -> RenderResultContext {
    RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: format!("http://daemon:8081/renders/scene_{}.png", index),
            generation_ms: 3000,
            tier: "scene".to_string(),
            scene_type: "exploration".to_string(),
        },
        subject: test_subject(),
    }
}

// =========================================================================
// AC1: image_cooldown_seconds field on SessionEventPayload
// =========================================================================

#[test]
fn session_event_payload_has_image_cooldown_field() {
    let payload = SessionEventPayload {
        event: "connect".to_string(),
        player_name: Some("Keith".to_string()),
        genre: Some("flickering_reach".to_string()),
        world: None,
        has_character: None,
        initial_state: None,
        css: None,
        narrator_verbosity: None,
        narrator_vocabulary: None,
        image_cooldown_seconds: Some(45),
    };
    assert_eq!(payload.image_cooldown_seconds, Some(45));
}

#[test]
fn session_event_payload_image_cooldown_defaults_to_none() {
    // Backward compat: old clients that don't send this field get None
    let json = r#"{
        "event": "connect",
        "player_name": "Keith",
        "genre": "flickering_reach"
    }"#;
    let payload: SessionEventPayload = serde_json::from_str(json).expect("deserialize");
    assert_eq!(
        payload.image_cooldown_seconds, None,
        "Missing field should deserialize as None for backward compatibility"
    );
}

// =========================================================================
// AC6: Wire format — field serializes/deserializes correctly
// =========================================================================

#[test]
fn image_cooldown_round_trips_through_json() {
    let payload = SessionEventPayload {
        event: "connect".to_string(),
        player_name: Some("Keith".to_string()),
        genre: Some("flickering_reach".to_string()),
        world: None,
        has_character: None,
        initial_state: None,
        css: None,
        narrator_verbosity: None,
        narrator_vocabulary: None,
        image_cooldown_seconds: Some(90),
    };
    let json = serde_json::to_string(&payload).expect("serialize");
    let roundtrip: SessionEventPayload = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(roundtrip.image_cooldown_seconds, Some(90));
}

#[test]
fn image_cooldown_null_in_json_deserializes_as_none() {
    let json = r#"{
        "event": "connect",
        "player_name": "Keith",
        "genre": "flickering_reach",
        "image_cooldown_seconds": null
    }"#;
    let payload: SessionEventPayload = serde_json::from_str(json).expect("deserialize");
    assert_eq!(payload.image_cooldown_seconds, None);
}

#[test]
fn image_cooldown_serialized_json_uses_correct_field_name() {
    let payload = SessionEventPayload {
        event: "connect".to_string(),
        player_name: None,
        genre: None,
        world: None,
        has_character: None,
        initial_state: None,
        css: None,
        narrator_verbosity: None,
        narrator_vocabulary: None,
        image_cooldown_seconds: Some(60),
    };
    let json_str = serde_json::to_string(&payload).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
    assert_eq!(
        json["image_cooldown_seconds"].as_u64(),
        Some(60),
        "Field must serialize as 'image_cooldown_seconds'"
    );
}

// =========================================================================
// AC2: Default cooldowns — 60s multiplayer, 30s solo
// =========================================================================

#[test]
fn default_cooldown_for_solo_is_30_seconds() {
    let throttle = ImagePacingThrottle::default_for_player_count(1);
    assert_eq!(
        throttle.cooldown_seconds(),
        30,
        "solo sessions should default to 30s cooldown"
    );
}

#[test]
fn default_cooldown_for_multiplayer_is_60_seconds() {
    let throttle = ImagePacingThrottle::default_for_player_count(2);
    assert_eq!(
        throttle.cooldown_seconds(),
        60,
        "multiplayer sessions should default to 60s cooldown"
    );
}

#[test]
fn default_cooldown_for_large_party_is_60_seconds() {
    let throttle = ImagePacingThrottle::default_for_player_count(5);
    assert_eq!(
        throttle.cooldown_seconds(),
        60,
        "large party sessions should default to 60s cooldown"
    );
}

#[test]
fn custom_cooldown_overrides_default() {
    let throttle = ImagePacingThrottle::with_cooldown(45);
    assert_eq!(
        throttle.cooldown_seconds(),
        45,
        "explicit cooldown should override default"
    );
}

// =========================================================================
// AC3: Throttle suppresses renders within cooldown window
// =========================================================================

#[test]
fn throttle_allows_first_render() {
    let throttle = ImagePacingThrottle::with_cooldown(60);
    assert!(
        throttle.should_allow(),
        "first render should always be allowed"
    );
}

#[test]
fn throttle_suppresses_immediate_second_render() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    // First render allowed
    assert!(throttle.should_allow());
    throttle.record_render();
    // Immediately after — should be suppressed
    assert!(
        !throttle.should_allow(),
        "render within cooldown window should be suppressed"
    );
}

#[test]
fn throttle_suppresses_rapid_sequence() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    assert!(throttle.should_allow());
    throttle.record_render();

    // Multiple rapid attempts should all be suppressed
    for _ in 0..5 {
        assert!(
            !throttle.should_allow(),
            "rapid renders should all be suppressed within cooldown"
        );
    }
}

// =========================================================================
// AC4: Cooldown resets after period expires
// =========================================================================

#[tokio::test]
async fn throttle_allows_after_cooldown_expires() {
    // Use a very short cooldown for test speed
    let mut throttle = ImagePacingThrottle::with_cooldown(1);
    assert!(throttle.should_allow());
    throttle.record_render();

    // Immediately after — suppressed
    assert!(!throttle.should_allow());

    // Wait for cooldown to expire
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Should allow again
    assert!(
        throttle.should_allow(),
        "render should be allowed after cooldown expires"
    );
}

#[test]
fn throttle_remaining_cooldown_decreases_over_time() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    throttle.record_render();

    let remaining = throttle.remaining_cooldown_seconds();
    assert!(
        remaining > 0 && remaining <= 60,
        "remaining cooldown should be between 0 and 60, got {}",
        remaining
    );
}

// =========================================================================
// AC5: DM force override bypasses throttle
// =========================================================================

#[test]
fn force_render_bypasses_throttle() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    assert!(throttle.should_allow());
    throttle.record_render();

    // Within cooldown — normal render suppressed
    assert!(!throttle.should_allow());

    // Force override — should allow regardless
    assert!(
        throttle.should_allow_forced(),
        "DM force should bypass throttle"
    );
}

#[test]
fn force_render_resets_cooldown_timer() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    throttle.record_render();

    // Force render
    assert!(throttle.should_allow_forced());
    throttle.record_render();

    // Cooldown should restart from the forced render
    assert!(
        !throttle.should_allow(),
        "cooldown should restart from forced render"
    );
}

// =========================================================================
// Integration: throttle wired into image broadcaster
// =========================================================================

#[tokio::test]
async fn broadcaster_with_throttle_suppresses_rapid_renders() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    // 60s cooldown — second render should be suppressed
    let throttle = ImagePacingThrottle::with_cooldown(60);
    let handle = spawn_image_broadcaster_with_throttle(render_rx, ws_tx, throttle);

    // Send two renders in rapid succession
    render_tx.send(test_render_context(1)).expect("send 1");
    render_tx.send(test_render_context(2)).expect("send 2");

    // First should arrive
    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("should receive first IMAGE")
        .expect("recv");
    assert!(matches!(msg, GameMessage::Image { .. }));

    // Give broadcaster time to process second render
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Second should NOT arrive (throttled)
    assert!(
        ws_rx.try_recv().is_err(),
        "second render should be throttled within cooldown"
    );

    handle.abort();
}

// Helper: spawn broadcaster with throttle (this function doesn't exist yet)
fn spawn_image_broadcaster_with_throttle(
    render_rx: broadcast::Receiver<RenderResultContext>,
    ws_tx: broadcast::Sender<GameMessage>,
    throttle: ImagePacingThrottle,
) -> tokio::task::JoinHandle<()> {
    // This references a function signature that must be implemented:
    // The real spawn_image_broadcaster should accept an optional throttle,
    // or there should be a variant that accepts one.
    sidequest_server::render_integration::spawn_image_broadcaster_with_throttle(
        render_rx, ws_tx, throttle,
    )
}

// =========================================================================
// Integration: cooldown updated mid-session via SESSION_EVENT
// =========================================================================

#[test]
fn throttle_cooldown_can_be_updated() {
    let mut throttle = ImagePacingThrottle::with_cooldown(60);
    assert_eq!(throttle.cooldown_seconds(), 60);

    throttle.set_cooldown(30);
    assert_eq!(
        throttle.cooldown_seconds(),
        30,
        "cooldown should be updatable mid-session"
    );
}

#[test]
fn throttle_zero_cooldown_disables_throttling() {
    let mut throttle = ImagePacingThrottle::with_cooldown(0);
    assert!(throttle.should_allow());
    throttle.record_render();

    // With 0 cooldown, next render should also be allowed
    assert!(
        throttle.should_allow(),
        "zero cooldown should effectively disable throttling"
    );
}
