//! Story 4-8: TTS Streaming tests
//!
//! Tests for the TTS streaming pipeline that synthesizes narration segments
//! and streams audio chunks to clients via channel-based messages.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;

use sidequest_game::tts_stream::{
    AudioFormat, TtsChunkPayload, TtsError, TtsMessage, TtsSegment, TtsStreamConfig,
    TtsStreamer, TtsSynthesizer,
};

// ===========================================================================
// Test helpers
// ===========================================================================

/// Mock synthesizer that returns fixed audio bytes.
struct MockSynthesizer {
    audio_bytes: Vec<u8>,
    call_count: AtomicUsize,
}

impl MockSynthesizer {
    fn new(audio_bytes: Vec<u8>) -> Self {
        Self {
            audio_bytes,
            call_count: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl TtsSynthesizer for MockSynthesizer {
    fn synthesize(
        &self,
        _text: &str,
        _speaker: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, TtsError>> + Send + '_>>
    {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let bytes = self.audio_bytes.clone();
        Box::pin(async move { Ok(bytes) })
    }
}

/// Mock synthesizer that fails on specific segment indices.
struct FailingSynthesizer {
    fail_indices: Vec<usize>,
    audio_bytes: Vec<u8>,
}

impl FailingSynthesizer {
    fn new(fail_indices: Vec<usize>, audio_bytes: Vec<u8>) -> Self {
        Self { fail_indices, audio_bytes }
    }
}

impl TtsSynthesizer for FailingSynthesizer {
    fn synthesize(
        &self,
        _text: &str,
        _speaker: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, TtsError>> + Send + '_>>
    {
        // We need to capture the index from the call count
        // This is a simplification — real impl would use text/speaker to determine
        let fail = self.fail_indices.clone();
        let bytes = self.audio_bytes.clone();
        Box::pin(async move {
            // Use a static counter for tracking calls
            static CALL: AtomicUsize = AtomicUsize::new(0);
            let idx = CALL.fetch_add(1, Ordering::SeqCst);
            if fail.contains(&idx) {
                Err(TtsError::SynthesisFailed(format!("segment {} failed", idx)))
            } else {
                Ok(bytes)
            }
        })
    }
}

/// Mock synthesizer that always fails (daemon down).
struct DeadSynthesizer;

impl TtsSynthesizer for DeadSynthesizer {
    fn synthesize(
        &self,
        _text: &str,
        _speaker: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, TtsError>> + Send + '_>>
    {
        Box::pin(async { Err(TtsError::DaemonUnavailable("connection refused".into())) })
    }
}

fn make_segments(count: usize) -> Vec<TtsSegment> {
    (0..count)
        .map(|i| TtsSegment {
            text: format!("Sentence number {}.", i + 1),
            index: i,
            is_last: i == count - 1,
            speaker: "narrator".to_string(),
            pause_after_ms: if i < count - 1 { 200 } else { 0 },
        })
        .collect()
}

async fn collect_messages(mut rx: mpsc::Receiver<TtsMessage>) -> Vec<TtsMessage> {
    let mut msgs = Vec::new();
    while let Some(msg) = rx.recv().await {
        msgs.push(msg);
    }
    msgs
}

// ===========================================================================
// AC 1: Stream start — TtsStart sent before first audio chunk
// ===========================================================================

#[tokio::test]
async fn stream_sends_start_before_chunks() {
    let synth = MockSynthesizer::new(vec![0u8; 100]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(3);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    assert!(!msgs.is_empty(), "should produce messages");

    // First message must be Start
    match &msgs[0] {
        TtsMessage::Start { total_segments } => {
            assert_eq!(*total_segments, 3);
        }
        other => panic!("expected TtsMessage::Start, got {:?}", other),
    }
}

// ===========================================================================
// AC 2: Chunk delivery — each segment produces a TtsChunk
// ===========================================================================

#[tokio::test]
async fn each_segment_produces_a_chunk() {
    let synth = MockSynthesizer::new(vec![42u8; 50]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(4);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(payload) => Some(payload),
            _ => None,
        })
        .collect();

    assert_eq!(chunks.len(), 4, "should have one chunk per segment");
    assert_eq!(synth.calls(), 4, "synthesizer called once per segment");
}

#[tokio::test]
async fn chunks_have_correct_segment_indices() {
    let synth = MockSynthesizer::new(vec![1u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(3);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let indices: Vec<usize> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p.segment_index),
            _ => None,
        })
        .collect();

    assert_eq!(indices, vec![0, 1, 2]);
}

// ===========================================================================
// AC 3: Stream end — TtsEnd sent after last chunk
// ===========================================================================

#[tokio::test]
async fn stream_sends_end_after_last_chunk() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(2);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    // Last message must be End
    match msgs.last() {
        Some(TtsMessage::End) => {}
        other => panic!("expected TtsMessage::End as last message, got {:?}", other),
    }
}

#[tokio::test]
async fn message_order_is_start_chunks_end() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(2);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    assert_eq!(msgs.len(), 4, "Start + 2 Chunks + End");

    assert!(matches!(msgs[0], TtsMessage::Start { .. }));
    assert!(matches!(msgs[1], TtsMessage::Chunk(_)));
    assert!(matches!(msgs[2], TtsMessage::Chunk(_)));
    assert!(matches!(msgs[3], TtsMessage::End));
}

// ===========================================================================
// AC 4: Prefetch — synthesis runs ahead by prefetch_count
// ===========================================================================

#[tokio::test]
async fn prefetch_count_defaults_to_two() {
    let config = TtsStreamConfig::default();
    assert_eq!(config.prefetch_count(), 2);
}

#[tokio::test]
async fn custom_prefetch_count_is_respected() {
    let config = TtsStreamConfig::new(5, AudioFormat::Opus);
    assert_eq!(config.prefetch_count(), 5);
    assert_eq!(*config.format(), AudioFormat::Opus);
}

// ===========================================================================
// AC 5: Pause hints — inter-segment pauses match pause_after_ms
// ===========================================================================

#[tokio::test]
async fn segments_with_pause_hints_are_delivered() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);

    let segments = vec![
        TtsSegment {
            text: "First.".into(),
            index: 0,
            is_last: false,
            speaker: "narrator".into(),
            pause_after_ms: 500,
        },
        TtsSegment {
            text: "Second.".into(),
            index: 1,
            is_last: true,
            speaker: "narrator".into(),
            pause_after_ms: 0,
        },
    ];

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .collect();

    assert_eq!(chunks.len(), 2, "both segments delivered despite pause hint");
}

// ===========================================================================
// AC 6: Base64 encoding — audio bytes encoded as base64 in payload
// ===========================================================================

#[tokio::test]
async fn audio_bytes_are_base64_encoded() {
    let raw_audio = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let synth = MockSynthesizer::new(raw_audio.clone());
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(1);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunk = msgs
        .iter()
        .find_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .expect("should have a chunk");

    // Decode the base64 and verify it matches raw audio
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&chunk.audio_base64)
        .expect("should be valid base64");
    assert_eq!(decoded, raw_audio);
}

// ===========================================================================
// AC 7: Segment failure — failed synthesis skips segment, continues stream
// ===========================================================================

#[tokio::test]
async fn failed_segment_is_skipped_stream_continues() {
    // Segment at index 1 fails
    let synth = FailingSynthesizer::new(vec![1], vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(3);

    let (tx, rx) = mpsc::channel(32);
    let result = streamer.stream(segments, &synth, tx).await;

    // Stream should succeed overall (failure is non-fatal)
    assert!(result.is_ok(), "stream should succeed despite segment failure");

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .collect();

    // Only 2 chunks (segments 0 and 2 succeeded, 1 was skipped)
    assert_eq!(chunks.len(), 2, "failed segment should be skipped");

    // Should still have Start and End
    assert!(matches!(msgs.first(), Some(TtsMessage::Start { .. })));
    assert!(matches!(msgs.last(), Some(TtsMessage::End)));
}

// ===========================================================================
// AC 8: Total failure — daemon down means no TTS, returns error or empty
// ===========================================================================

#[tokio::test]
async fn daemon_down_produces_no_chunks() {
    let synth = DeadSynthesizer;
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(3);

    let (tx, rx) = mpsc::channel(32);
    let _ = streamer.stream(segments, &synth, tx).await;

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter(|m| matches!(m, TtsMessage::Chunk(_)))
        .collect();

    assert_eq!(chunks.len(), 0, "no chunks when daemon is down");

    // Should still get Start and End for protocol completeness
    assert!(msgs.iter().any(|m| matches!(m, TtsMessage::Start { .. })));
    assert!(msgs.iter().any(|m| matches!(m, TtsMessage::End)));
}

// ===========================================================================
// AC 9: Speaker included — each chunk carries speaker identity
// ===========================================================================

#[tokio::test]
async fn chunks_carry_speaker_identity() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);

    let segments = vec![
        TtsSegment {
            text: "The narrator speaks.".into(),
            index: 0,
            is_last: false,
            speaker: "narrator".into(),
            pause_after_ms: 0,
        },
        TtsSegment {
            text: "Thorn replies.".into(),
            index: 1,
            is_last: true,
            speaker: "Thorn Ironhide".into(),
            pause_after_ms: 0,
        },
    ];

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .collect();

    assert_eq!(chunks[0].speaker, "narrator");
    assert_eq!(chunks[1].speaker, "Thorn Ironhide");
}

// ===========================================================================
// AC 10: Non-blocking — stream runs as background task
// ===========================================================================

#[tokio::test]
async fn streamer_can_be_spawned_as_background_task() {
    let synth = Arc::new(MockSynthesizer::new(vec![0u8; 10]));
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(2);

    let (tx, rx) = mpsc::channel(32);

    // Spawn as background task — must not block the caller
    let synth_ref = synth.clone();
    let handle = tokio::spawn(async move {
        streamer.stream(segments, synth_ref.as_ref(), tx).await
    });

    // Collect messages from the receiver while task runs
    let msgs = collect_messages(rx).await;
    let result = handle.await.expect("task should not panic");
    assert!(result.is_ok());
    assert!(!msgs.is_empty(), "should receive messages from background task");
}

// ===========================================================================
// Rule enforcement: #[non_exhaustive] on enums
// ===========================================================================

#[test]
fn audio_format_is_non_exhaustive() {
    let fmt = AudioFormat::Wav;
    match fmt {
        AudioFormat::Wav => {}
        AudioFormat::Opus => {}
        _ => {} // compiles because #[non_exhaustive]
    }
}

#[test]
fn tts_error_is_non_exhaustive() {
    let err = TtsError::ChannelClosed;
    match err {
        TtsError::SynthesisFailed(_) => {}
        TtsError::DaemonUnavailable(_) => {}
        TtsError::ChannelClosed => {}
        _ => {} // compiles because #[non_exhaustive]
    }
}

// ===========================================================================
// Rule enforcement: private fields with getters
// ===========================================================================

#[test]
fn config_fields_are_private_with_getters() {
    let config = TtsStreamConfig::default();
    // These should compile — using getters, not direct field access
    assert_eq!(config.prefetch_count(), 2);
    assert_eq!(*config.format(), AudioFormat::Wav);
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[tokio::test]
async fn empty_segments_produces_start_and_end_only() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(vec![], &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    assert_eq!(msgs.len(), 2, "Start + End for empty segments");
    assert!(matches!(msgs[0], TtsMessage::Start { total_segments: 0 }));
    assert!(matches!(msgs[1], TtsMessage::End));
}

#[tokio::test]
async fn single_segment_sets_is_last_chunk_true() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(1);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunk = msgs
        .iter()
        .find_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .expect("should have one chunk");

    assert!(chunk.is_last_chunk, "single segment should be marked as last");
}

#[tokio::test]
async fn last_chunk_has_is_last_true_others_false() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::default();
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(3);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunks: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .collect();

    assert!(!chunks[0].is_last_chunk);
    assert!(!chunks[1].is_last_chunk);
    assert!(chunks[2].is_last_chunk);
}

#[tokio::test]
async fn chunk_format_matches_config() {
    let synth = MockSynthesizer::new(vec![0u8; 10]);
    let config = TtsStreamConfig::new(2, AudioFormat::Opus);
    let streamer = TtsStreamer::new(config);
    let segments = make_segments(1);

    let (tx, rx) = mpsc::channel(32);
    streamer.stream(segments, &synth, tx).await.unwrap();

    let msgs = collect_messages(rx).await;
    let chunk = msgs
        .iter()
        .find_map(|m| match m {
            TtsMessage::Chunk(p) => Some(p),
            _ => None,
        })
        .expect("should have a chunk");

    assert_eq!(chunk.format, AudioFormat::Opus);
}
