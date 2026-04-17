//! sidequest-test-support — shared test harness (story 40-1).
//!
//! This crate is the single source of truth for the runtime test harness used
//! across the sidequest-api workspace. It exists to eradicate two anti-patterns
//! catalogued in Epic 40:
//!
//! 1. Source-grep `.contains(...)` assertions on stringified log output. These
//!    tests pass whenever any matching substring appears — they do not verify
//!    that the correct span/event fired with the correct fields. Replacement:
//!    [`SpanCaptureLayer`] with a typed field-query API.
//!
//! 2. Concrete [`sidequest_agents::client::ClaudeClient`] types baked into
//!    production signatures. Tests cannot substitute a mock without firing up
//!    the real Claude CLI subprocess. Replacement: [`ClaudeLike`] trait, with
//!    production sites taking `Arc<dyn ClaudeLike>` and tests supplying
//!    [`MockClaudeClient`].
//!
//! This file is intentionally minimal at story 40-1 RED. Dev will populate the
//! modules during the GREEN phase. See `README.md` for the canonical recipe.
