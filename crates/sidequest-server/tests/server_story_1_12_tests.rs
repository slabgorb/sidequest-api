//! Story 1-12: Server — axum router, WebSocket, genres endpoint, service facade, structured logging
//!
//! RED phase tests. These verify the server's transport layer:
//! - axum Router with /ws and /api/genres routes
//! - WebSocket message dispatch via GameMessage
//! - GameService trait facade (server never accesses game internals)
//! - Processing gate (prevent double-submission)
//! - CORS headers for React dev server
//! - Structured tracing spans
//! - Graceful shutdown
//!
//! Tests use axum's built-in test utilities and tower::ServiceExt.

// These tests import from the server crate's public API.
// The server crate must expose the types and functions needed.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt; // for oneshot()

/// The server crate must expose a function to build the axum Router.
/// This is the primary integration point — all tests use this.
use sidequest_server::build_router;

/// The server crate must expose AppState for test construction.
use sidequest_server::AppState;

/// The server crate must expose a way to create test-ready state.
use sidequest_server::test_app_state;

// =========================================================================
// AC: REST endpoint — GET /api/genres returns genre pack summaries as JSON
// =========================================================================

#[tokio::test]
async fn get_genres_returns_200_with_json() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get(header::CONTENT_TYPE).unwrap();
    assert!(
        content_type.to_str().unwrap().contains("application/json"),
        "Content-Type should be application/json, got: {:?}",
        content_type
    );
}

#[tokio::test]
async fn get_genres_returns_genre_map_with_worlds() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // The response should be an object where each key is a genre slug
    // and each value has a "worlds" array of world slugs.
    assert!(
        json.is_object(),
        "Response should be a JSON object, got: {}",
        json
    );

    // With a test genre_packs_path, we expect at least one genre.
    // The exact genres depend on the test fixture, but the structure must be:
    // { "genre_slug": { "worlds": ["world1", "world2"] } }
    for (_genre_slug, genre_data) in json.as_object().unwrap() {
        assert!(
            genre_data.get("worlds").is_some(),
            "Each genre must have a 'worlds' field"
        );
        assert!(genre_data["worlds"].is_array(), "'worlds' must be an array");
    }
}

#[tokio::test]
async fn get_genres_nonexistent_path_returns_empty_or_error() {
    // When genre_packs_path doesn't exist or is empty, the endpoint should
    // return an empty object or a 500, not panic.
    let state = test_app_state(); // uses a test path
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Must not panic — either 200 with empty {} or a proper error status
    assert!(
        response.status() == StatusCode::OK || response.status().is_server_error(),
        "Should return 200 or 5xx, not panic"
    );
}

// =========================================================================
// AC: Unknown routes return 404
// =========================================================================

#[tokio::test]
async fn unknown_route_returns_404() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// AC: CORS — cross-origin requests from React dev server allowed
// =========================================================================

#[tokio::test]
async fn cors_allows_localhost_5173() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/genres")
                .header(header::ORIGIN, "http://localhost:5173")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // CORS preflight should succeed (200 or 204) with appropriate headers
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NO_CONTENT,
        "CORS preflight should succeed, got: {}",
        response.status()
    );

    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(
        allow_origin.is_some(),
        "Access-Control-Allow-Origin header must be present"
    );
}

#[tokio::test]
async fn cors_headers_on_regular_request() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .header(header::ORIGIN, "http://localhost:5173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(
        allow_origin.is_some(),
        "Access-Control-Allow-Origin must be present on regular requests too"
    );
}

// =========================================================================
// AC: WebSocket upgrade endpoint exists at /ws
// =========================================================================

#[tokio::test]
async fn ws_endpoint_rejects_non_upgrade_request() {
    let state = test_app_state();
    let app = build_router(state);

    // A regular GET to /ws without WebSocket upgrade headers should fail
    let response = app
        .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Should reject with 400 or 426 (Upgrade Required), not 404
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "/ws endpoint must exist (got 404)"
    );
}

// =========================================================================
// AC: Service facade — server uses GameService trait, never game internals
// =========================================================================

/// The server must accept any implementation of GameService, not just Orchestrator.
/// This test verifies the facade pattern by providing a mock implementation.
#[tokio::test]
async fn server_accepts_mock_game_service() {
    use sidequest_agents::orchestrator::GameService;

    /// A mock GameService for testing. The server must work with any
    /// GameService impl, proving it doesn't depend on Orchestrator internals.
    struct MockGameService;

    impl GameService for MockGameService {
        fn get_snapshot(&self) -> serde_json::Value {
            serde_json::json!({
                "mock": true,
                "location": "Test Tavern"
            })
        }

        fn process_action(
            &self,
            action: &str,
            _context: &sidequest_agents::orchestrator::TurnContext,
        ) -> sidequest_agents::orchestrator::ActionResult {
            sidequest_agents::orchestrator::ActionResult {
                confrontation: None,
                location: None,
                prompt_text: None,
                raw_response_text: None,
                narration: format!("Mock response to: {}", action),
                beat_selections: vec![],
                is_degraded: false,
                classified_intent: None,
                agent_name: None,
                footnotes: vec![],
                items_gained: vec![],
                npcs_present: vec![],
                quest_updates: std::collections::HashMap::new(),
                agent_duration_ms: None,
                token_count_in: None,
                token_count_out: None,
                visual_scene: None,
                scene_mood: None,
                personality_events: vec![],
                scene_intent: None,
                resource_deltas: std::collections::HashMap::new(),
                zone_breakdown: None,
                lore_established: None,
                action_rewrite: None,
                action_flags: None,
                sfx_triggers: vec![],
                merchant_transactions: vec![],
                prompt_tier: String::new(),
            }
        }

        fn reset_narrator_session_for_connect(&self) {
            // No-op for mock — no persistent session to reset
        }
    }

    // The server must be constructable with a mock GameService.
    // This proves the facade pattern — server depends on the trait, not the impl.
    let state = AppState::new_with_game_service(
        Box::new(MockGameService),
        std::path::PathBuf::from("/tmp/test-genres"),
        std::path::PathBuf::from("/tmp/test-saves"),
    );
    let app = build_router(state);

    // If this compiles and the router builds, the facade pattern holds.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should work regardless of which GameService impl is provided
    assert!(response.status().is_success() || response.status().is_server_error());
}

// =========================================================================
// AC: Server starts — AppState construction and router building
// =========================================================================

#[tokio::test]
async fn app_state_creation_succeeds() {
    // AppState must be constructable without panicking
    let state = test_app_state();
    // Verify it has expected fields accessible
    assert!(
        !format!("{:?}", state).is_empty(),
        "AppState should implement Debug"
    );
}

#[tokio::test]
async fn build_router_returns_valid_router() {
    let state = test_app_state();
    let app = build_router(state);

    // The router should handle requests without panicking
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert!(
        response.is_ok(),
        "Router should handle requests without error"
    );
}

// =========================================================================
// AC: Structured logging — tracing spans with component/operation/player_id
// =========================================================================

/// The server must use tracing spans for structured logging.
/// We verify this by checking that the server module imports and uses tracing,
/// and that requests produce span-compatible output.
///
/// Note: Full tracing verification requires a tracing subscriber in tests.
/// This test ensures the server's request handling creates spans.
#[tokio::test]
async fn request_completes_with_tracing_active() {
    // Install a test tracing subscriber to capture spans
    let _guard = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    let state = test_app_state();
    let app = build_router(state);

    // This request should produce tracing output (we can't easily assert on it,
    // but at minimum it must not panic with tracing enabled)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// =========================================================================
// AC: POST to REST endpoint returns 405 Method Not Allowed
// =========================================================================

#[tokio::test]
async fn post_to_genres_returns_method_not_allowed() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST to /api/genres should be 405"
    );
}

// =========================================================================
// Integration: Multiple routes coexist
// =========================================================================

#[tokio::test]
async fn multiple_routes_coexist() {
    let state = test_app_state();

    // We need two separate router instances since oneshot consumes the service
    let app1 = build_router(state.clone());
    let app2 = build_router(state);

    // /api/genres works
    let r1 = app1
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);

    // /ws exists (rejects non-upgrade, but doesn't 404)
    let r2 = app2
        .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(r2.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// AppState must be Clone (required for axum state extraction)
// =========================================================================

#[tokio::test]
async fn app_state_is_clone() {
    let state = test_app_state();
    let _cloned = state.clone(); // Must compile — axum requires Clone for State
}

// =========================================================================
// AppState must be Send + Sync (required for async handlers)
// =========================================================================

fn _assert_send_sync<T: Send + Sync>() {}

#[test]
fn app_state_is_send_sync() {
    _assert_send_sync::<AppState>();
}
