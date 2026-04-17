#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

//! sidequest-test-support — shared test harness (story 40-1).

mod mock_client;
mod span_capture;

pub use mock_client::{MockClaudeClient, RecordedCall};
pub use span_capture::{CapturedEvent, CapturedSpan, SpanCapture, SpanCaptureLayer};

/// Re-export the canonical [`ClaudeLike`] trait from `sidequest-agents` so tests
/// can import either path and get the same trait.
///
/// The trait itself lives next to [`sidequest_agents::client::ClaudeClient`]
/// because that is the concrete type it abstracts. This crate provides the
/// test-only [`MockClaudeClient`] impl plus the [`SpanCaptureLayer`] harness.
pub use sidequest_agents::client::ClaudeLike;
