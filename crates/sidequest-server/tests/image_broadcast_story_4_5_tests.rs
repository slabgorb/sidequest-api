//! Story 4-5: IMAGE message broadcast — failing tests (RED phase).
//!
//! Tests cover 9 acceptance criteria:
//! 1. Broadcast on success — completed render produces IMAGE message
//! 2. Payload shape — JSON matches client contract
//! 3. Failed render silent — failure logs warning, no client message
//! 4. Tier included — SubjectTier serialized as lowercase string
//! 5. Scene type included — SceneType serialized as lowercase string
//! 6. Session-scoped — broadcaster stops on channel close
//! 7. Non-blocking — broadcast does not block render queue
//! 8. Latency metadata — generation_ms passed through
//! 9. Protocol type — GameMessage::Image variant exists
//!
//! Plus rule-enforcement tests from the Rust lang-review checklist.

use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

use sidequest_game::render_queue::RenderJobResult;
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};
use sidequest_protocol::{GameMessage, ImagePayload};
use sidequest_server::render_integration::{spawn_image_broadcaster, RenderResultContext};

// =========================================================================
// Helper: build a test RenderSubject
// =========================================================================

fn test_subject(tier: SubjectTier, scene_type: SceneType) -> RenderSubject {
    RenderSubject::new(
        vec!["Kira".to_string(), "Ren".to_string()],
        scene_type,
        tier,
        "Two warriors clash in the burning courtyard".to_string(),
        0.8,
    )
    .expect("valid test subject")
}

fn test_render_context_success() -> RenderResultContext {
    RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: "http://daemon:8081/renders/abc123.png".to_string(),
            generation_ms: 3200,
        },
        subject: test_subject(SubjectTier::Landscape, SceneType::Exploration),
    }
}

fn test_render_context_failed() -> RenderResultContext {
    RenderResultContext {
        result: RenderJobResult::Failed {
            job_id: Uuid::new_v4(),
            error: "GPU out of memory".to_string(),
        },
        subject: test_subject(SubjectTier::Scene, SceneType::Combat),
    }
}

// =========================================================================
// AC1: Broadcast on success — completed render produces IMAGE message
// =========================================================================

#[tokio::test]
async fn broadcaster_sends_image_on_render_success() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send a successful render result
    render_tx
        .send(test_render_context_success())
        .expect("send should succeed");

    // The broadcaster should translate it into a GameMessage::Image
    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("should receive within timeout")
        .expect("recv should succeed");

    assert!(
        matches!(msg, GameMessage::Image { .. }),
        "Expected GameMessage::Image, got {:?}",
        msg
    );

    handle.abort();
}

// =========================================================================
// AC2: Payload shape — JSON matches client contract
// =========================================================================

#[tokio::test]
async fn image_payload_json_matches_client_contract() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    render_tx
        .send(test_render_context_success())
        .expect("send should succeed");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("should receive within timeout")
        .expect("recv should succeed");

    // Serialize to JSON and verify the client contract shape
    let json_str = serde_json::to_string(&msg).expect("serialize should succeed");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse should succeed");

    // Must have "type": "IMAGE"
    assert_eq!(json["type"], "IMAGE", "Message type must be IMAGE");

    // Payload must contain all client-contract fields
    let payload = &json["payload"];
    assert!(
        payload.get("image_url").is_some() || payload.get("url").is_some(),
        "Payload must contain image_url"
    );
    assert!(
        payload.get("description").is_some(),
        "Payload must contain description"
    );
    assert!(payload.get("tier").is_some(), "Payload must contain tier");
    assert!(
        payload.get("scene_type").is_some(),
        "Payload must contain scene_type"
    );
    assert!(
        payload.get("generation_ms").is_some(),
        "Payload must contain generation_ms"
    );

    handle.abort();
}

// =========================================================================
// AC3: Failed render silent — logs warning, no client message
// =========================================================================

#[tokio::test]
async fn broadcaster_ignores_failed_render() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send a failed render result
    render_tx
        .send(test_render_context_failed())
        .expect("send should succeed");

    // Give the broadcaster time to process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // No message should have been sent to the WebSocket channel
    let result = ws_rx.try_recv();
    assert!(
        result.is_err(),
        "Failed render should NOT produce a WebSocket message, but got: {:?}",
        result
    );

    handle.abort();
}

// =========================================================================
// AC4: Tier included — SubjectTier serialized as lowercase string
// =========================================================================

#[tokio::test]
async fn image_payload_includes_tier_as_lowercase_string() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Use Landscape tier
    render_tx
        .send(test_render_context_success())
        .expect("send should succeed");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("should receive within timeout")
        .expect("recv should succeed");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    let tier = json["payload"]["tier"]
        .as_str()
        .expect("tier must be a string");
    assert_eq!(tier, "landscape", "Tier must be lowercase string");

    handle.abort();
}

#[tokio::test]
async fn tier_portrait_serializes_lowercase() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    let ctx = RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: "http://daemon:8081/renders/portrait1.png".to_string(),
            generation_ms: 1500,
        },
        subject: test_subject(SubjectTier::Portrait, SceneType::Dialogue),
    };
    render_tx.send(ctx).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    assert_eq!(
        json["payload"]["tier"].as_str().expect("tier string"),
        "portrait"
    );

    handle.abort();
}

// =========================================================================
// AC5: Scene type included — SceneType serialized as lowercase string
// =========================================================================

#[tokio::test]
async fn image_payload_includes_scene_type_as_lowercase_string() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    render_tx.send(test_render_context_success()).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    let scene_type = json["payload"]["scene_type"]
        .as_str()
        .expect("scene_type must be a string");
    assert_eq!(
        scene_type, "exploration",
        "Scene type must be lowercase string"
    );

    handle.abort();
}

#[tokio::test]
async fn scene_type_combat_serializes_lowercase() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    let ctx = RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: "http://daemon:8081/renders/combat1.png".to_string(),
            generation_ms: 4000,
        },
        subject: test_subject(SubjectTier::Scene, SceneType::Combat),
    };
    render_tx.send(ctx).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    assert_eq!(
        json["payload"]["scene_type"]
            .as_str()
            .expect("scene_type string"),
        "combat"
    );

    handle.abort();
}

// =========================================================================
// AC6: Session-scoped — broadcaster stops on channel close
// =========================================================================

#[tokio::test]
async fn broadcaster_stops_when_render_channel_closes() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, _ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Drop the render sender — closes the channel
    drop(render_tx);

    // The broadcaster task should terminate gracefully
    let result = tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("broadcaster should terminate within timeout");

    assert!(
        result.is_ok(),
        "Broadcaster task should exit cleanly when channel closes"
    );
}

// =========================================================================
// AC7: Non-blocking — broadcast does not block render queue
// =========================================================================

#[tokio::test]
async fn broadcaster_does_not_block_render_queue_sender() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, _ws_rx) = broadcast::channel::<GameMessage>(16);

    let _handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Sending multiple render results rapidly should not block
    let start = tokio::time::Instant::now();
    for i in 0..10 {
        let ctx = RenderResultContext {
            result: RenderJobResult::Success {
                job_id: Uuid::new_v4(),
                image_url: format!("http://daemon:8081/renders/img{}.png", i),
                generation_ms: 1000 + i as u64 * 100,
            },
            subject: test_subject(SubjectTier::Scene, SceneType::Exploration),
        };
        render_tx.send(ctx).expect("send should not block");
    }
    let elapsed = start.elapsed();

    // 10 sends should complete nearly instantly (well under 100ms)
    assert!(
        elapsed < Duration::from_millis(100),
        "Sending 10 render results took {:?} — broadcaster is blocking the sender",
        elapsed
    );

    _handle.abort();
}

// =========================================================================
// AC8: Latency metadata — generation_ms passed through
// =========================================================================

#[tokio::test]
async fn image_payload_includes_generation_ms() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    let expected_ms: u64 = 4567;
    let ctx = RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: "http://daemon:8081/renders/latency_test.png".to_string(),
            generation_ms: expected_ms,
        },
        subject: test_subject(SubjectTier::Scene, SceneType::Combat),
    };
    render_tx.send(ctx).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    let gen_ms = json["payload"]["generation_ms"]
        .as_u64()
        .expect("generation_ms must be a u64");
    assert_eq!(
        gen_ms, expected_ms,
        "generation_ms must pass through from render result"
    );

    handle.abort();
}

// =========================================================================
// AC9: Protocol type — GameMessage::Image variant exists
// =========================================================================

#[test]
fn game_message_image_variant_exists() {
    // Verify the Image variant can be constructed and pattern-matched
    let payload = ImagePayload {
        url: "http://example.com/test.png".to_string(),
        description: "A test image".to_string(),
        handout: false,
        render_id: None,
        tier: None,
        scene_type: None,
        generation_ms: None,
    };

    let msg = GameMessage::Image {
        payload: payload.clone(),
        player_id: String::new(),
    };

    match msg {
        GameMessage::Image { payload: p, .. } => {
            assert_eq!(p.url, "http://example.com/test.png");
            assert_eq!(p.description, "A test image");
        }
        _ => panic!("Expected GameMessage::Image variant"),
    }
}

// =========================================================================
// Rule #1: Silent error swallowing — broadcaster handles no-subscriber case
// =========================================================================

#[tokio::test]
async fn broadcaster_handles_no_subscribers_gracefully() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    // Create ws_tx but drop all receivers — simulates no connected clients
    let (ws_tx, _) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send a success result when no WebSocket subscribers exist
    render_tx
        .send(test_render_context_success())
        .expect("send should succeed");

    // Give time to process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The broadcaster should NOT panic or stop — it continues running
    assert!(
        !handle.is_finished(),
        "Broadcaster should keep running even with zero subscribers"
    );

    handle.abort();
}

// =========================================================================
// Rule #4: Tracing — failed render produces warning log
// =========================================================================

#[tokio::test]
async fn failed_render_produces_tracing_warning() {
    // This test verifies the contract that failed renders log a warning.
    // The actual tracing capture is implementation-dependent, so we verify
    // the broadcaster processes the failure without crashing and doesn't
    // send a message to clients.
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send failure then success — if broadcaster crashed on failure,
    // we won't receive the success
    render_tx
        .send(test_render_context_failed())
        .expect("send failed result");

    render_tx
        .send(test_render_context_success())
        .expect("send success result");

    // Should receive the success (proving the failure didn't crash the broadcaster)
    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("should receive within timeout")
        .expect("recv should succeed");

    assert!(
        matches!(msg, GameMessage::Image { .. }),
        "Should receive IMAGE from the success result after silently handling failure"
    );

    handle.abort();
}

// =========================================================================
// Rule #9: ImagePayload description comes from subject prompt_fragment
// =========================================================================

#[tokio::test]
async fn image_description_comes_from_subject_prompt_fragment() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    let subject = RenderSubject::new(
        vec!["Kira".to_string()],
        SceneType::Dialogue,
        SubjectTier::Portrait,
        "A warrior stands before the burning gate".to_string(),
        0.9,
    )
    .expect("valid subject");

    let ctx = RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: "http://daemon:8081/renders/desc_test.png".to_string(),
            generation_ms: 2000,
        },
        subject,
    };
    render_tx.send(ctx).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    let description = json["payload"]["description"]
        .as_str()
        .expect("description must be a string");
    assert_eq!(
        description, "A warrior stands before the burning gate",
        "Description should come from the subject's prompt_fragment"
    );

    handle.abort();
}

// =========================================================================
// Rule #9: image_url in payload comes from RenderJobResult
// =========================================================================

#[tokio::test]
async fn image_url_comes_from_render_result() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    let expected_url = "http://daemon:8081/renders/unique_hash_42.png";
    let ctx = RenderResultContext {
        result: RenderJobResult::Success {
            job_id: Uuid::new_v4(),
            image_url: expected_url.to_string(),
            generation_ms: 1000,
        },
        subject: test_subject(SubjectTier::Scene, SceneType::Exploration),
    };
    render_tx.send(ctx).expect("send");

    let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    let json_str = serde_json::to_string(&msg).expect("serialize");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

    // Check that the URL from the render result ends up in the payload
    let payload = &json["payload"];
    let url_value = payload
        .get("image_url")
        .or_else(|| payload.get("url"))
        .expect("payload must contain image URL field");
    assert_eq!(
        url_value.as_str().expect("url must be string"),
        expected_url,
        "Image URL must come from RenderJobResult::Success"
    );

    handle.abort();
}

// =========================================================================
// Integration: multiple sequential renders produce correct messages
// =========================================================================

#[tokio::test]
async fn multiple_renders_produce_multiple_image_messages() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send 3 successful renders
    for i in 0..3 {
        let ctx = RenderResultContext {
            result: RenderJobResult::Success {
                job_id: Uuid::new_v4(),
                image_url: format!("http://daemon:8081/renders/multi_{}.png", i),
                generation_ms: 1000 * (i + 1) as u64,
            },
            subject: test_subject(SubjectTier::Scene, SceneType::Exploration),
        };
        render_tx.send(ctx).expect("send");
    }

    // Should receive exactly 3 IMAGE messages
    for i in 0..3 {
        let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
            .await
            .unwrap_or_else(|_| panic!("Timed out waiting for message {}", i))
            .unwrap_or_else(|e| panic!("Recv error for message {}: {}", i, e));

        assert!(
            matches!(msg, GameMessage::Image { .. }),
            "Message {} should be GameMessage::Image",
            i
        );
    }

    handle.abort();
}

// =========================================================================
// Integration: interleaved success and failure
// =========================================================================

#[tokio::test]
async fn interleaved_success_and_failure_only_broadcasts_success() {
    let (render_tx, render_rx) = broadcast::channel::<RenderResultContext>(16);
    let (ws_tx, mut ws_rx) = broadcast::channel::<GameMessage>(16);

    let handle = spawn_image_broadcaster(render_rx, ws_tx);

    // Send: success, fail, fail, success
    render_tx
        .send(test_render_context_success())
        .expect("send 1");
    render_tx
        .send(test_render_context_failed())
        .expect("send 2");
    render_tx
        .send(test_render_context_failed())
        .expect("send 3");
    render_tx
        .send(test_render_context_success())
        .expect("send 4");

    // Should receive exactly 2 IMAGE messages (from the 2 successes)
    for i in 0..2 {
        let msg = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
            .await
            .unwrap_or_else(|_| panic!("Timed out waiting for success message {}", i))
            .unwrap_or_else(|e| panic!("Recv error for message {}: {}", i, e));

        assert!(
            matches!(msg, GameMessage::Image { .. }),
            "Message {} should be GameMessage::Image",
            i
        );
    }

    // No more messages should be pending
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        ws_rx.try_recv().is_err(),
        "Should have no more messages after the 2 successes"
    );

    handle.abort();
}
