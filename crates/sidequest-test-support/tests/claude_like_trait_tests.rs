//! Story 40-1 RED: ClaudeLike trait exists and covers production send methods.
//!
//! These tests fail to compile today because `sidequest_test_support::ClaudeLike`
//! does not exist. Dev's GREEN phase defines the trait so that
//! `sidequest_agents::client::ClaudeClient` and `MockClaudeClient` both
//! implement it, allowing `Arc<dyn ClaudeLike>` to be substituted for
//! concrete `ClaudeClient` at production sites.
//!
//! Trait surface requirements (derived from SM Assessment ACs and
//! `sidequest_agents::client::ClaudeClient`):
//! - `send_with_model(prompt: &str, model: &str) -> Result<ClaudeResponse, ClaudeClientError>`
//! - `send_with_session(prompt, model, session_id, system_prompt, allowed_tools, env_vars)
//!    -> Result<ClaudeResponse, ClaudeClientError>`
//!
//! The trait must be object-safe so `Arc<dyn ClaudeLike>` compiles.

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
