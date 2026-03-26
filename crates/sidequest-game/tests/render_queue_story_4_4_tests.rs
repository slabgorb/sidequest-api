//! Story 4-4: Render queue — async image generation queue with hash-based cache dedup
//!
//! RED phase — these tests exercise the RenderQueue, content hashing, dedup,
//! tier-to-dimensions mapping, and configuration validation.
//! They will panic/fail until Dev implements:
//!   - render_queue.rs: RenderQueue::spawn(), enqueue(), job_status(), cache_len(), shutdown()
//!   - compute_content_hash() for dedup
//!   - tier_to_dimensions() for image sizing
//!   - Background worker loop calling DaemonClient::render()
//!   - Cache TTL eviction
//!   - Result broadcasting via tokio::sync::broadcast

use std::time::Duration;

use sidequest_game::render_queue::{
    compute_content_hash, tier_to_dimensions, EnqueueResult, ImageDimensions, QueueError,
    RenderJobResult, RenderQueue, RenderQueueConfig, RenderStatus, DEFAULT_CACHE_TTL,
    MAX_QUEUE_DEPTH,
};
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};

// ============================================================================
// Test fixtures
// ============================================================================

fn make_subject(entities: &[&str], scene_type: SceneType, tier: SubjectTier) -> RenderSubject {
    RenderSubject::new(
        entities.iter().map(|s| s.to_string()).collect(),
        scene_type,
        tier,
        format!("{} in a dramatic scene", entities.join(" and ")),
        0.8,
    )
    .expect("Test fixture: valid RenderSubject")
}

fn combat_subject() -> RenderSubject {
    make_subject(
        &["Grak the Destroyer", "Mira Shadowstep"],
        SceneType::Combat,
        SubjectTier::Scene,
    )
}

fn landscape_subject() -> RenderSubject {
    make_subject(
        &[],
        SceneType::Exploration,
        SubjectTier::Landscape,
    )
}

fn portrait_subject() -> RenderSubject {
    make_subject(
        &["Old Sage Theron"],
        SceneType::Dialogue,
        SubjectTier::Portrait,
    )
}

fn abstract_subject() -> RenderSubject {
    make_subject(
        &[],
        SceneType::Exploration,
        SubjectTier::Abstract,
    )
}

fn default_config() -> RenderQueueConfig {
    RenderQueueConfig::default()
}

fn small_queue_config() -> RenderQueueConfig {
    RenderQueueConfig::new(2, 4, Duration::from_secs(60)).expect("Valid small config")
}

// ============================================================================
// AC: RenderQueueConfig — validated construction
// ============================================================================

#[test]
fn config_default_has_reasonable_values() {
    let config = RenderQueueConfig::default();
    assert!(
        config.queue_depth() > 0 && config.queue_depth() <= MAX_QUEUE_DEPTH,
        "Default queue_depth should be in (0, MAX_QUEUE_DEPTH], got {}",
        config.queue_depth()
    );
    assert!(
        config.result_buffer() > 0,
        "Default result_buffer should be > 0, got {}",
        config.result_buffer()
    );
    assert!(
        !config.cache_ttl().is_zero(),
        "Default cache_ttl should be non-zero"
    );
    assert_eq!(
        config.cache_ttl(),
        DEFAULT_CACHE_TTL,
        "Default cache_ttl should match DEFAULT_CACHE_TTL"
    );
}

#[test]
fn config_new_accepts_valid_values() {
    let config = RenderQueueConfig::new(32, 16, Duration::from_secs(120));
    assert!(config.is_some(), "Valid config values should be accepted");

    let config = config.unwrap();
    assert_eq!(config.queue_depth(), 32);
    assert_eq!(config.result_buffer(), 16);
    assert_eq!(config.cache_ttl(), Duration::from_secs(120));
}

#[test]
fn config_new_rejects_zero_queue_depth() {
    let config = RenderQueueConfig::new(0, 16, Duration::from_secs(60));
    assert!(
        config.is_none(),
        "queue_depth=0 should be rejected"
    );
}

#[test]
fn config_new_rejects_excessive_queue_depth() {
    let config = RenderQueueConfig::new(MAX_QUEUE_DEPTH + 1, 16, Duration::from_secs(60));
    assert!(
        config.is_none(),
        "queue_depth exceeding MAX_QUEUE_DEPTH should be rejected"
    );
}

#[test]
fn config_new_accepts_max_queue_depth() {
    let config = RenderQueueConfig::new(MAX_QUEUE_DEPTH, 16, Duration::from_secs(60));
    assert!(
        config.is_some(),
        "queue_depth=MAX_QUEUE_DEPTH should be accepted"
    );
}

#[test]
fn config_new_rejects_zero_result_buffer() {
    let config = RenderQueueConfig::new(32, 0, Duration::from_secs(60));
    assert!(
        config.is_none(),
        "result_buffer=0 should be rejected"
    );
}

#[test]
fn config_new_rejects_zero_cache_ttl() {
    let config = RenderQueueConfig::new(32, 16, Duration::ZERO);
    assert!(
        config.is_none(),
        "cache_ttl=zero should be rejected"
    );
}

// ============================================================================
// AC: Content hashing — deterministic dedup key
// ============================================================================

#[test]
fn content_hash_is_deterministic() {
    let subject = combat_subject();
    let hash1 = compute_content_hash(&subject);
    let hash2 = compute_content_hash(&subject);
    assert_eq!(hash1, hash2, "Same subject should produce same hash");
}

#[test]
fn content_hash_differs_for_different_entities() {
    let subject_a = make_subject(
        &["Grak the Destroyer"],
        SceneType::Combat,
        SubjectTier::Scene,
    );
    let subject_b = make_subject(
        &["Mira Shadowstep"],
        SceneType::Combat,
        SubjectTier::Scene,
    );
    assert_ne!(
        compute_content_hash(&subject_a),
        compute_content_hash(&subject_b),
        "Different entities should produce different hashes"
    );
}

#[test]
fn content_hash_differs_for_different_scene_types() {
    let subject_combat = make_subject(
        &["Grak the Destroyer"],
        SceneType::Combat,
        SubjectTier::Scene,
    );
    let subject_dialogue = make_subject(
        &["Grak the Destroyer"],
        SceneType::Dialogue,
        SubjectTier::Scene,
    );
    assert_ne!(
        compute_content_hash(&subject_combat),
        compute_content_hash(&subject_dialogue),
        "Different scene types should produce different hashes"
    );
}

#[test]
fn content_hash_differs_for_different_tiers() {
    let subject_portrait = make_subject(
        &["Grak the Destroyer"],
        SceneType::Combat,
        SubjectTier::Portrait,
    );
    let subject_scene = make_subject(
        &["Grak the Destroyer"],
        SceneType::Combat,
        SubjectTier::Scene,
    );
    assert_ne!(
        compute_content_hash(&subject_portrait),
        compute_content_hash(&subject_scene),
        "Different tiers should produce different hashes"
    );
}

#[test]
fn content_hash_ignores_prompt_fragment_differences() {
    // Two subjects with same entities/scene_type/tier but different prompt text
    let subject_a = RenderSubject::new(
        vec!["Grak".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "Grak swings his axe".to_string(),
        0.8,
    )
    .unwrap();
    let subject_b = RenderSubject::new(
        vec!["Grak".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "Grak charges with fury".to_string(),
        0.8,
    )
    .unwrap();

    assert_eq!(
        compute_content_hash(&subject_a),
        compute_content_hash(&subject_b),
        "Content hash should ignore prompt_fragment differences — dedup is on entities+scene+tier"
    );
}

#[test]
fn content_hash_is_case_insensitive_on_entities() {
    let subject_lower = RenderSubject::new(
        vec!["grak".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "combat".to_string(),
        0.8,
    )
    .unwrap();
    let subject_upper = RenderSubject::new(
        vec!["GRAK".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "combat".to_string(),
        0.8,
    )
    .unwrap();

    assert_eq!(
        compute_content_hash(&subject_lower),
        compute_content_hash(&subject_upper),
        "Content hash should be case-insensitive on entity names"
    );
}

// ============================================================================
// AC: Tier dimensions — Portrait tall, Scene square, Landscape wide
// ============================================================================

#[test]
fn portrait_tier_has_tall_aspect_ratio() {
    let dims = tier_to_dimensions(&SubjectTier::Portrait);
    assert!(
        dims.height > dims.width,
        "Portrait should be taller than wide. Got {}x{}",
        dims.width,
        dims.height
    );
}

#[test]
fn landscape_tier_has_wide_aspect_ratio() {
    let dims = tier_to_dimensions(&SubjectTier::Landscape);
    assert!(
        dims.width > dims.height,
        "Landscape should be wider than tall. Got {}x{}",
        dims.width,
        dims.height
    );
}

#[test]
fn scene_tier_has_square_or_near_square_aspect() {
    let dims = tier_to_dimensions(&SubjectTier::Scene);
    let ratio = dims.width as f64 / dims.height as f64;
    assert!(
        (0.9..=1.1).contains(&ratio),
        "Scene should be roughly square. Got {}x{} (ratio {:.2})",
        dims.width,
        dims.height,
        ratio
    );
}

#[test]
fn abstract_tier_has_square_dimensions() {
    let dims = tier_to_dimensions(&SubjectTier::Abstract);
    let ratio = dims.width as f64 / dims.height as f64;
    assert!(
        (0.9..=1.1).contains(&ratio),
        "Abstract should be roughly square. Got {}x{} (ratio {:.2})",
        dims.width,
        dims.height,
        ratio
    );
}

#[test]
fn all_tiers_have_positive_dimensions() {
    let tiers = [
        SubjectTier::Portrait,
        SubjectTier::Scene,
        SubjectTier::Landscape,
        SubjectTier::Abstract,
    ];
    for tier in &tiers {
        let dims = tier_to_dimensions(tier);
        assert!(dims.width > 0, "Width must be positive for {:?}", tier);
        assert!(dims.height > 0, "Height must be positive for {:?}", tier);
    }
}

// ============================================================================
// AC: Async enqueue — returns immediately with job ID or dedup
// ============================================================================

#[tokio::test]
async fn enqueue_returns_queued_with_job_id() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject = combat_subject();

    let result = queue.enqueue(subject, "oil_painting", "flux-schnell").await;
    assert!(result.is_ok(), "Enqueue should succeed on non-full queue");

    match result.unwrap() {
        EnqueueResult::Queued { job_id } => {
            // job_id should be a valid UUID (non-nil)
            assert_ne!(
                job_id,
                uuid::Uuid::nil(),
                "Job ID should be a non-nil UUID"
            );
        }
        EnqueueResult::Deduplicated { .. } => {
            panic!("First enqueue should not be deduplicated");
        }
        _ => panic!("Unexpected EnqueueResult variant"),
    }

    queue.shutdown().await;
}

#[tokio::test]
async fn enqueue_is_non_blocking() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject = combat_subject();

    // Enqueue should return near-instantly (not wait for rendering)
    let start = std::time::Instant::now();
    let _result = queue.enqueue(subject, "oil_painting", "flux-schnell").await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "Enqueue should be non-blocking but took {:?}",
        elapsed
    );

    queue.shutdown().await;
}

// ============================================================================
// AC: Dedup — same content hash returns Deduplicated
// ============================================================================

#[tokio::test]
async fn duplicate_subject_returns_deduplicated() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject = combat_subject();

    // First enqueue — should be Queued
    let first = queue
        .enqueue(subject.clone(), "oil_painting", "flux-schnell")
        .await
        .unwrap();
    let first_id = match first {
        EnqueueResult::Queued { job_id } => job_id,
        _ => panic!("First enqueue should be Queued"),
    };

    // Second enqueue with identical subject — should be Deduplicated
    let second = queue
        .enqueue(subject, "oil_painting", "flux-schnell")
        .await
        .unwrap();
    match second {
        EnqueueResult::Deduplicated { original_id } => {
            assert_eq!(
                original_id, first_id,
                "Deduplicated result should reference the first job's ID"
            );
        }
        EnqueueResult::Queued { .. } => {
            panic!("Duplicate subject should be deduplicated, not queued again");
        }
        _ => panic!("Unexpected EnqueueResult variant"),
    }

    queue.shutdown().await;
}

#[tokio::test]
async fn different_subjects_not_deduplicated() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject_a = combat_subject();
    let subject_b = landscape_subject();

    let first = queue
        .enqueue(subject_a, "oil_painting", "flux-schnell")
        .await
        .unwrap();
    assert!(
        matches!(first, EnqueueResult::Queued { .. }),
        "First enqueue should be Queued"
    );

    let second = queue
        .enqueue(subject_b, "oil_painting", "flux-schnell")
        .await
        .unwrap();
    assert!(
        matches!(second, EnqueueResult::Queued { .. }),
        "Different subject should be Queued, not Deduplicated"
    );

    queue.shutdown().await;
}

// ============================================================================
// AC: Queue depth — rejects when full (backpressure)
// ============================================================================

#[tokio::test]
async fn queue_rejects_when_full() {
    // Small queue that fills quickly
    let config = RenderQueueConfig::new(1, 4, Duration::from_secs(60)).unwrap();
    let queue = RenderQueue::spawn(config, |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });

    // Fill the queue with distinct subjects
    let subject_a = combat_subject();
    let subject_b = landscape_subject();
    let subject_c = portrait_subject();

    let _ = queue.enqueue(subject_a, "oil_painting", "flux-schnell").await;
    let _ = queue.enqueue(subject_b, "oil_painting", "flux-schnell").await;

    // At some point, the queue should reject with QueueError::Full
    // We try several enqueues to trigger backpressure
    let mut got_full = false;
    for i in 0..10 {
        let subject = RenderSubject::new(
            vec![format!("Entity{}", i)],
            SceneType::Exploration,
            SubjectTier::Landscape,
            format!("scene {}", i),
            0.8,
        )
        .unwrap();
        match queue.enqueue(subject, "oil_painting", "flux-schnell").await {
            Err(QueueError::Full) => {
                got_full = true;
                break;
            }
            _ => continue,
        }
    }

    assert!(
        got_full,
        "Queue with depth=1 should eventually reject with QueueError::Full"
    );

    queue.shutdown().await;
}

// ============================================================================
// AC: Job status tracking
// ============================================================================

#[tokio::test]
async fn job_status_returns_queued_after_enqueue() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject = combat_subject();

    let result = queue
        .enqueue(subject, "oil_painting", "flux-schnell")
        .await
        .unwrap();

    if let EnqueueResult::Queued { job_id } = result {
        let status = queue.job_status(job_id).await;
        assert!(
            status.is_some(),
            "Job should be findable by ID after enqueue"
        );
        // Immediately after enqueue, status should be Queued or InProgress
        let status = status.unwrap();
        assert!(
            matches!(status, RenderStatus::Queued | RenderStatus::InProgress),
            "Job should be Queued or InProgress right after enqueue, got {:?}",
            status
        );
    }

    queue.shutdown().await;
}

#[tokio::test]
async fn job_status_returns_none_for_unknown_id() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let unknown_id = uuid::Uuid::new_v4();

    let status = queue.job_status(unknown_id).await;
    assert!(
        status.is_none(),
        "Unknown job ID should return None, got {:?}",
        status
    );

    queue.shutdown().await;
}

// ============================================================================
// AC: Failure handling — daemon error produces Failed, not panic
// ============================================================================

// Note: Full failure handling tests require a mock DaemonClient.
// These tests verify the type system supports failure reporting.

#[test]
fn render_status_failed_carries_error_message() {
    let status = RenderStatus::Failed {
        error: "daemon GPU out of memory".to_string(),
    };
    match status {
        RenderStatus::Failed { error } => {
            assert_eq!(error, "daemon GPU out of memory");
        }
        _ => panic!("Expected RenderStatus::Failed"),
    }
}

#[test]
fn render_job_result_failed_carries_job_id_and_error() {
    let job_id = uuid::Uuid::new_v4();
    let result = RenderJobResult::Failed {
        job_id,
        error: "timeout".to_string(),
    };
    match result {
        RenderJobResult::Failed {
            job_id: id,
            error,
        } => {
            assert_eq!(id, job_id);
            assert_eq!(error, "timeout");
        }
        _ => panic!("Expected RenderJobResult::Failed"),
    }
}

#[test]
fn render_job_result_success_carries_all_fields() {
    let job_id = uuid::Uuid::new_v4();
    let result = RenderJobResult::Success {
        job_id,
        image_url: "/renders/abc123.png".to_string(),
        generation_ms: 3500,
    };
    match result {
        RenderJobResult::Success {
            job_id: id,
            image_url,
            generation_ms,
        } => {
            assert_eq!(id, job_id);
            assert_eq!(image_url, "/renders/abc123.png");
            assert_eq!(generation_ms, 3500);
        }
        _ => panic!("Expected RenderJobResult::Success"),
    }
}

// ============================================================================
// AC: Cache update — completed renders stored with hash key
// ============================================================================

#[tokio::test]
async fn cache_len_increases_after_enqueue() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let initial_len = queue.cache_len().await;

    let subject = combat_subject();
    let _ = queue.enqueue(subject, "oil_painting", "flux-schnell").await;

    let new_len = queue.cache_len().await;
    assert!(
        new_len > initial_len,
        "Cache should grow after enqueue. Was {}, now {}",
        initial_len,
        new_len
    );

    queue.shutdown().await;
}

#[tokio::test]
async fn duplicate_enqueue_does_not_increase_cache_len() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let subject = combat_subject();

    let _ = queue.enqueue(subject.clone(), "oil_painting", "flux-schnell").await;
    let len_after_first = queue.cache_len().await;

    let _ = queue.enqueue(subject, "oil_painting", "flux-schnell").await;
    let len_after_second = queue.cache_len().await;

    assert_eq!(
        len_after_first, len_after_second,
        "Duplicate enqueue should not increase cache size"
    );

    queue.shutdown().await;
}

// ============================================================================
// AC: Non-blocking — game loop never blocks waiting for render
// ============================================================================

#[tokio::test]
async fn multiple_enqueues_complete_without_blocking() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    let start = std::time::Instant::now();

    // Enqueue 10 different subjects rapidly
    for i in 0..10 {
        let subject = RenderSubject::new(
            vec![format!("Entity{}", i)],
            SceneType::Exploration,
            SubjectTier::Landscape,
            format!("scene {}", i),
            0.8,
        )
        .unwrap();
        let _ = queue.enqueue(subject, "oil_painting", "flux-schnell").await;
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "10 enqueues should complete in under 1s (non-blocking). Took {:?}",
        elapsed
    );

    queue.shutdown().await;
}

// ============================================================================
// Rule #2: #[non_exhaustive] on public enums
// ============================================================================

#[test]
fn render_status_variants_require_wildcard() {
    let statuses = vec![
        RenderStatus::Queued,
        RenderStatus::InProgress,
        RenderStatus::Complete {
            image_url: "test.png".to_string(),
            generation_ms: 100,
        },
        RenderStatus::Failed {
            error: "test".to_string(),
        },
        RenderStatus::Deduplicated {
            original_id: uuid::Uuid::nil(),
        },
    ];
    for status in &statuses {
        match status {
            RenderStatus::Queued
            | RenderStatus::InProgress
            | RenderStatus::Complete { .. }
            | RenderStatus::Failed { .. }
            | RenderStatus::Deduplicated { .. } => {}
            // This wildcard arm is required because of #[non_exhaustive]
            _ => panic!("Unexpected RenderStatus variant"),
        }
    }
    assert_eq!(statuses.len(), 5, "Should have 5 RenderStatus variants");
}

#[test]
fn enqueue_result_variants_require_wildcard() {
    let results = vec![
        EnqueueResult::Queued {
            job_id: uuid::Uuid::nil(),
        },
        EnqueueResult::Deduplicated {
            original_id: uuid::Uuid::nil(),
        },
    ];
    for result in &results {
        match result {
            EnqueueResult::Queued { .. } | EnqueueResult::Deduplicated { .. } => {}
            _ => panic!("Unexpected EnqueueResult variant"),
        }
    }
    assert_eq!(results.len(), 2, "Should have 2 EnqueueResult variants");
}

#[test]
fn queue_error_variants_require_wildcard() {
    let errors = vec![QueueError::Full, QueueError::Closed];
    for error in &errors {
        match error {
            QueueError::Full | QueueError::Closed => {}
            _ => panic!("Unexpected QueueError variant"),
        }
    }
    assert_eq!(errors.len(), 2, "Should have 2 QueueError variants");
}

#[test]
fn render_job_result_variants_require_wildcard() {
    let results = vec![
        RenderJobResult::Success {
            job_id: uuid::Uuid::nil(),
            image_url: "test.png".to_string(),
            generation_ms: 100,
        },
        RenderJobResult::Failed {
            job_id: uuid::Uuid::nil(),
            error: "test".to_string(),
        },
    ];
    for result in &results {
        match result {
            RenderJobResult::Success { .. } | RenderJobResult::Failed { .. } => {}
            _ => panic!("Unexpected RenderJobResult variant"),
        }
    }
    assert_eq!(results.len(), 2, "Should have 2 RenderJobResult variants");
}

// ============================================================================
// Rule #5: Validated constructors — RenderQueueConfig rejects invalid input
// ============================================================================

#[test]
fn config_constructor_boundary_values() {
    // Minimum valid values
    let min = RenderQueueConfig::new(1, 1, Duration::from_millis(1));
    assert!(min.is_some(), "Minimum valid config should be accepted");

    // Maximum queue depth
    let max = RenderQueueConfig::new(MAX_QUEUE_DEPTH, 1, Duration::from_millis(1));
    assert!(max.is_some(), "MAX_QUEUE_DEPTH should be accepted");

    // Just above max
    let over = RenderQueueConfig::new(MAX_QUEUE_DEPTH + 1, 1, Duration::from_millis(1));
    assert!(over.is_none(), "Over MAX_QUEUE_DEPTH should be rejected");
}

// ============================================================================
// Rule #9: Private fields with getters — config fields not directly accessible
// ============================================================================

#[test]
fn config_fields_accessed_through_getters() {
    let config = RenderQueueConfig::new(32, 16, Duration::from_secs(120)).unwrap();

    // Verify all getters return expected values
    assert_eq!(config.queue_depth(), 32);
    assert_eq!(config.result_buffer(), 16);
    assert_eq!(config.cache_ttl(), Duration::from_secs(120));
}

// ============================================================================
// Rule #1: QueueError Display — no silent error swallowing
// ============================================================================

#[test]
fn queue_error_display_is_descriptive() {
    let full = QueueError::Full;
    let closed = QueueError::Closed;

    let full_msg = format!("{}", full);
    let closed_msg = format!("{}", closed);

    assert!(
        !full_msg.is_empty(),
        "QueueError::Full display should not be empty"
    );
    assert!(
        !closed_msg.is_empty(),
        "QueueError::Closed display should not be empty"
    );
    assert_ne!(
        full_msg, closed_msg,
        "Different errors should have different messages"
    );
}

#[test]
fn queue_error_implements_std_error() {
    fn assert_error<E: std::error::Error>() {}
    assert_error::<QueueError>();
}

// ============================================================================
// Rule #15: Bounded queue — MAX_QUEUE_DEPTH prevents unbounded growth
// ============================================================================

#[test]
fn max_queue_depth_is_reasonable() {
    assert!(
        MAX_QUEUE_DEPTH > 0,
        "MAX_QUEUE_DEPTH must be positive"
    );
    assert!(
        MAX_QUEUE_DEPTH <= 10_000,
        "MAX_QUEUE_DEPTH should not be unreasonably large. Got {}",
        MAX_QUEUE_DEPTH
    );
}

#[test]
fn default_cache_ttl_is_reasonable() {
    assert!(
        DEFAULT_CACHE_TTL >= Duration::from_secs(60),
        "DEFAULT_CACHE_TTL should be at least 60s"
    );
    assert!(
        DEFAULT_CACHE_TTL <= Duration::from_secs(3600),
        "DEFAULT_CACHE_TTL should not exceed 1 hour"
    );
}

// ============================================================================
// AC: RenderStatus lifecycle — Complete carries result data
// ============================================================================

#[test]
fn render_status_complete_carries_image_url_and_timing() {
    let status = RenderStatus::Complete {
        image_url: "/renders/scene_abc123.png".to_string(),
        generation_ms: 2500,
    };
    match status {
        RenderStatus::Complete {
            image_url,
            generation_ms,
        } => {
            assert_eq!(image_url, "/renders/scene_abc123.png");
            assert_eq!(generation_ms, 2500);
        }
        _ => panic!("Expected RenderStatus::Complete"),
    }
}

#[test]
fn render_status_deduplicated_references_original() {
    let original_id = uuid::Uuid::new_v4();
    let status = RenderStatus::Deduplicated { original_id };
    match status {
        RenderStatus::Deduplicated { original_id: id } => {
            assert_eq!(id, original_id);
        }
        _ => panic!("Expected RenderStatus::Deduplicated"),
    }
}

// ============================================================================
// AC: ImageDimensions — struct fields accessible
// ============================================================================

#[test]
fn image_dimensions_fields_are_accessible() {
    let dims = ImageDimensions {
        width: 512,
        height: 768,
    };
    assert_eq!(dims.width, 512);
    assert_eq!(dims.height, 768);
}

#[test]
fn image_dimensions_supports_equality() {
    let a = ImageDimensions {
        width: 512,
        height: 768,
    };
    let b = ImageDimensions {
        width: 512,
        height: 768,
    };
    let c = ImageDimensions {
        width: 768,
        height: 512,
    };
    assert_eq!(a, b, "Same dimensions should be equal");
    assert_ne!(a, c, "Different dimensions should not be equal");
}

// ============================================================================
// Edge: hash consistency with empty entities
// ============================================================================

#[test]
fn content_hash_works_with_empty_entities() {
    let subject = RenderSubject::new(
        vec![],
        SceneType::Exploration,
        SubjectTier::Landscape,
        "a vast empty desert".to_string(),
        0.5,
    )
    .unwrap();

    let hash = compute_content_hash(&subject);
    // Should not panic and should produce a valid hash
    assert_ne!(hash, 0, "Hash of subject with empty entities should be non-zero");
}

#[test]
fn content_hash_with_many_entities_is_stable() {
    let entities: Vec<String> = (0..20).map(|i| format!("Entity{}", i)).collect();
    let subject = RenderSubject::new(
        entities,
        SceneType::Combat,
        SubjectTier::Scene,
        "massive battle".to_string(),
        0.9,
    )
    .unwrap();

    let hash1 = compute_content_hash(&subject);
    let hash2 = compute_content_hash(&subject);
    assert_eq!(hash1, hash2, "Hash should be stable across calls");
}

// ============================================================================
// Edge: Entity order should not affect hash
// ============================================================================

#[test]
fn content_hash_is_order_independent_on_entities() {
    let subject_ab = RenderSubject::new(
        vec!["Alpha".to_string(), "Beta".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "scene".to_string(),
        0.8,
    )
    .unwrap();
    let subject_ba = RenderSubject::new(
        vec!["Beta".to_string(), "Alpha".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "scene".to_string(),
        0.8,
    )
    .unwrap();

    assert_eq!(
        compute_content_hash(&subject_ab),
        compute_content_hash(&subject_ba),
        "Entity order should not affect content hash (same scene, different order)"
    );
}

// ============================================================================
// Shutdown safety
// ============================================================================

#[tokio::test]
async fn shutdown_completes_without_panic() {
    let queue = RenderQueue::spawn(default_config(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    // Enqueue some work then shut down
    let _ = queue
        .enqueue(combat_subject(), "oil_painting", "flux-schnell")
        .await;
    queue.shutdown().await;
    // If we reach here without panic, shutdown is clean
}

#[tokio::test]
async fn spawn_with_default_config_succeeds() {
    let queue = RenderQueue::spawn(RenderQueueConfig::default(), |_prompt, _style| async { Ok(("test.png".to_string(), 100)) });
    // Queue should be usable immediately after spawn
    let len = queue.cache_len().await;
    assert_eq!(len, 0, "Fresh queue should have empty cache");
    queue.shutdown().await;
}
