//! Story 40-1: pins the `ClaudeLike` object-safety contract and verifies both
//! `ClaudeClient` and `MockClaudeClient` implement it.
//!
//! The trait surface (defined in `sidequest_agents::client`):
//! - `send_with_model(prompt: &str, model: &str) -> Result<ClaudeResponse, ClaudeClientError>`
//! - `send_with_session(prompt, model, session_id, system_prompt, allowed_tools, env_vars)
//!    -> Result<ClaudeResponse, ClaudeClientError>`
//!
//! Object safety is load-bearing: production sites take `Arc<dyn ClaudeLike>`
//! so tests can substitute a `MockClaudeClient` without spawning a real Claude
//! CLI subprocess. Any change that breaks object safety (adds generics,
//! returns `Self`, takes `self` by value) will fail these tests at compile
//! time.

use std::sync::Arc;

use sidequest_agents::client::ClaudeClient;
use sidequest_test_support::{ClaudeLike, MockClaudeClient};

#[test]
fn claude_client_implements_claude_like() {
    // Compile-time check: ClaudeClient must implement ClaudeLike.
    fn assert_impl<T: ClaudeLike + ?Sized>() {}
    assert_impl::<ClaudeClient>();
}

#[test]
fn mock_claude_client_implements_claude_like() {
    // Compile-time check: MockClaudeClient must implement ClaudeLike.
    fn assert_impl<T: ClaudeLike + ?Sized>() {}
    assert_impl::<MockClaudeClient>();
}

#[test]
fn claude_like_is_object_safe_as_arc_dyn() {
    // Compile-time check: Arc<dyn ClaudeLike> must be constructable.
    // Any trait method taking `self` by value, generic type parameters,
    // or `Self` in an argument/return position would break object safety.
    let mock = MockClaudeClient::new();
    let arc: Arc<dyn ClaudeLike> = Arc::new(mock);
    // Verify we can clone the Arc (a minimum usability check on the trait object).
    let cloned = Arc::clone(&arc);
    assert_eq!(Arc::strong_count(&arc), 2);
    drop(cloned);
    assert_eq!(Arc::strong_count(&arc), 1);
}
