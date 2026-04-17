//! Consolidated integration tests: agent_infra

#[path = "agent_infra/agent_impl_story_1_11_tests.rs"]
#[allow(clippy::all)]
mod agent_impl_story_1_11_tests;

#[path = "agent_infra/agent_infrastructure_tests.rs"]
#[allow(clippy::all)]
mod agent_infrastructure_tests;

#[path = "agent_infra/exercise_tracker_story_3_5_tests.rs"]
#[allow(clippy::all)]
mod exercise_tracker_story_3_5_tests;

#[path = "agent_infra/otel_injection_story_21_4_tests.rs"]
#[allow(clippy::all)]
mod otel_injection_story_21_4_tests;

#[path = "agent_infra/telemetry_story_3_1_tests.rs"]
#[allow(clippy::all)]
mod telemetry_story_3_1_tests;

