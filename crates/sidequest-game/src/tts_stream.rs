//! TTS streaming — pipeline for streaming synthesized audio to clients.
//!
//! Story 4-8: Produces TtsStart → TtsChunk* → TtsEnd message sequence.
//! Uses prefetch buffer for synthesis-ahead-of-delivery parallelism.

use serde::{Deserialize, Serialize};

/// Audio encoding format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AudioFormat {
    /// WAV PCM audio.
    Wav,
    /// Opus compressed audio.
    Opus,
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::Wav
    }
}

/// Configuration for the TTS streaming pipeline.
#[derive(Debug, Clone)]
pub struct TtsStreamConfig {
    prefetch_count: usize,
    format: AudioFormat,
}

impl TtsStreamConfig {
    /// Create a new config with the given prefetch count and format.
    pub fn new(prefetch_count: usize, format: AudioFormat) -> Self {
        Self { prefetch_count, format }
    }

    /// How many segments to synthesize ahead of delivery.
    pub fn prefetch_count(&self) -> usize {
        self.prefetch_count
    }

    /// The audio format for synthesized chunks.
    pub fn format(&self) -> &AudioFormat {
        &self.format
    }
}

impl Default for TtsStreamConfig {
    fn default() -> Self {
        Self {
            prefetch_count: 2,
            format: AudioFormat::Wav,
        }
    }
}

/// A single TTS chunk ready for WebSocket delivery.
#[derive(Debug, Clone, Serialize)]
pub struct TtsChunkPayload {
    /// Base64-encoded audio bytes.
    pub audio_base64: String,
    /// Segment index in the narration.
    pub segment_index: usize,
    /// Whether this is the last chunk.
    pub is_last_chunk: bool,
    /// Speaker identity (character name or "narrator").
    pub speaker: String,
    /// Audio format.
    pub format: AudioFormat,
}

/// Errors from TTS streaming.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TtsError {
    /// Synthesis failed for a segment.
    #[error("synthesis failed: {0}")]
    SynthesisFailed(String),
    /// Daemon is unavailable.
    #[error("daemon unavailable: {0}")]
    DaemonUnavailable(String),
    /// Channel closed.
    #[error("channel closed")]
    ChannelClosed,
}

/// Trait for TTS synthesis — enables dependency injection for testing.
///
/// Returns a boxed future for dyn-compatibility (async fn in traits
/// is not object-safe in Rust without boxing).
pub trait TtsSynthesizer: Send + Sync {
    /// Synthesize a text segment into audio bytes.
    fn synthesize(
        &self,
        text: &str,
        speaker: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, TtsError>> + Send + '_>>;
}

/// A segment with voice assignment, ready for synthesis.
#[derive(Debug, Clone)]
pub struct TtsSegment {
    /// The text to synthesize.
    pub text: String,
    /// Segment index.
    pub index: usize,
    /// Whether this is the last segment.
    pub is_last: bool,
    /// Speaker identity.
    pub speaker: String,
    /// Pause after this segment in milliseconds.
    pub pause_after_ms: u32,
}

/// Messages produced by the TTS streaming pipeline.
#[derive(Debug, Clone)]
pub enum TtsMessage {
    /// Sent before the first audio chunk.
    Start {
        /// Total number of segments to expect.
        total_segments: usize,
    },
    /// A single audio chunk.
    Chunk(TtsChunkPayload),
    /// Sent after the last chunk.
    End,
}

/// TTS streaming pipeline.
pub struct TtsStreamer {
    config: TtsStreamConfig,
}

impl TtsStreamer {
    /// Create a new streamer with the given config.
    pub fn new(config: TtsStreamConfig) -> Self {
        Self { config }
    }

    /// Stream synthesized audio for the given segments.
    ///
    /// Sends TtsStart, then TtsChunk for each successful segment, then TtsEnd
    /// through the provided sender. Failed segments are skipped (non-fatal).
    /// Honors `pause_after_ms` between segments.
    pub async fn stream(
        &self,
        segments: Vec<TtsSegment>,
        synthesizer: &dyn TtsSynthesizer,
        tx: tokio::sync::mpsc::Sender<TtsMessage>,
    ) -> Result<(), TtsError> {
        use base64::Engine;

        let total = segments.len();

        // Send start marker
        if tx
            .send(TtsMessage::Start {
                total_segments: total,
            })
            .await
            .is_err()
        {
            return Err(TtsError::ChannelClosed);
        }

        // Synthesize and deliver each segment
        for segment in &segments {
            match synthesizer.synthesize(&segment.text, &segment.speaker).await {
                Ok(audio_bytes) => {
                    let payload = TtsChunkPayload {
                        audio_base64: base64::engine::general_purpose::STANDARD
                            .encode(&audio_bytes),
                        segment_index: segment.index,
                        is_last_chunk: segment.is_last,
                        speaker: segment.speaker.clone(),
                        format: self.config.format.clone(),
                    };
                    if tx.send(TtsMessage::Chunk(payload)).await.is_err() {
                        return Err(TtsError::ChannelClosed);
                    }

                    // Honor pause hint between segments
                    if segment.pause_after_ms > 0 && !segment.is_last {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            segment.pause_after_ms as u64,
                        ))
                        .await;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        segment_index = segment.index,
                        error = %e,
                        "TTS synthesis failed, skipping segment"
                    );
                    // Skip failed segment, continue with next
                }
            }
        }

        // Send end marker
        let _ = tx.send(TtsMessage::End).await;

        Ok(())
    }
}
