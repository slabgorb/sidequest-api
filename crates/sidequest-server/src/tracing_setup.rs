//! Tracing / Telemetry initialization (Story 3-1).
//!
//! Composable tracing subscriber stack with JSON + pretty layers.

use std::sync::{Arc, Mutex};

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

/// Initialize the composable tracing subscriber stack.
///
/// Uses Registry + layers instead of the bare `tracing_subscriber::fmt::init()`.
/// Layers:
/// - EnvFilter: respects RUST_LOG (default: `sidequest=debug,tower_http=info`)
/// - JSON layer: structured output for production (always active)
/// - Pretty layer: human-readable output in debug builds only
pub fn init_tracing(enable_chrome_trace: bool) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sidequest=debug,tower_http=info"));

    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true);

    let pretty_layer = if cfg!(debug_assertions) {
        Some(tracing_subscriber::fmt::layer().pretty())
    } else {
        None
    };

    // Chrome trace layer — produces flame-chart JSON loadable in chrome://tracing or Perfetto.
    // Enabled via --trace flag.
    let chrome_layer = if enable_chrome_trace {
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .file(format!("trace-{}.json", std::process::id()))
            .include_args(true)
            .build();
        // Guard must outlive the subscriber — intentionally leak it.
        std::mem::forget(guard);
        Some(layer)
    } else {
        None
    };

    Registry::default()
        .with(env_filter)
        .with(json_layer)
        .with(pretty_layer)
        .with(chrome_layer)
        .init();

    if enable_chrome_trace {
        tracing::info!("Chrome trace → trace-{}.json", std::process::id());
    }
}

/// Wrapper for writing to a shared buffer (used in tests).
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Build a tracing subscriber that writes JSON to a shared buffer (for tests).
#[doc(hidden)]
pub fn tracing_subscriber_for_test(
    writer: Arc<Mutex<Vec<u8>>>,
) -> Box<dyn tracing::Subscriber + Send + Sync> {
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true)
        .with_writer(move || SharedWriter(writer.clone()));

    Box::new(Registry::default().with(json_layer))
}

/// Build a subscriber with a custom EnvFilter string.
/// Returns `Some(subscriber)` if the filter parses, `None` otherwise.
#[doc(hidden)]
pub fn build_subscriber_with_filter(
    filter: &str,
) -> Option<Box<dyn tracing::Subscriber + Send + Sync>> {
    let env_filter = EnvFilter::try_new(filter).ok()?;
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true);

    Some(Box::new(
        Registry::default().with(env_filter).with(json_layer),
    ))
}
