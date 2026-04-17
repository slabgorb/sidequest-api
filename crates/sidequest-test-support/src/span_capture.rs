//! Typed span/event capture for tests — replaces source-grep `.contains(...)`.
//!
//! Install [`SpanCaptureLayer`] on a `tracing_subscriber::Registry`; query
//! the returned [`SpanCapture`] handle after the code under test has run.
//! Tests assert on typed field values, not stringified log output.
//!
//! ```
//! use sidequest_test_support::SpanCaptureLayer;
//! use tracing::subscriber::with_default;
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_subscriber::Registry;
//!
//! let (layer, capture) = SpanCaptureLayer::new();
//! let subscriber = Registry::default().with(layer);
//! with_default(subscriber, || {
//!     tracing::info!(event_name = "encounter.beat_applied", beat_id = "merge");
//! });
//! let events = capture.events_by_name("encounter.beat_applied");
//! assert_eq!(events[0].field_str("beat_id"), Some("merge".to_string()));
//! ```

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A single captured span — name plus all recorded field values.
#[derive(Debug, Clone, Default)]
pub struct CapturedSpan {
    id: u64,
    name: String,
    fields: Vec<(String, FieldValue)>,
}

/// A single captured event (a `tracing::info!` / `warn!` / etc. call).
///
/// Name is resolved from the `event_name` field if present (conventional in
/// sidequest-api tracing calls), else from the event metadata's name.
#[derive(Debug, Clone, Default)]
pub struct CapturedEvent {
    name: String,
    fields: Vec<(String, FieldValue)>,
}

/// Typed field value — lets tests assert on actual types, not stringified
/// `Debug` output.
#[derive(Debug, Clone)]
enum FieldValue {
    Str(String),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Debug(String),
}

impl CapturedSpan {
    /// String field, if present and recorded as a string.
    pub fn field_str(&self, name: &str) -> Option<String> {
        self.find(name).and_then(|v| match v {
            FieldValue::Str(s) => Some(s.clone()),
            FieldValue::Debug(s) => Some(s.clone()),
            _ => None,
        })
    }

    /// i64 field, if present and recorded as an integer. Values recorded as
    /// `u64` are converted losslessly when representable.
    pub fn field_i64(&self, name: &str) -> Option<i64> {
        self.find(name).and_then(|v| match v {
            FieldValue::I64(n) => Some(*n),
            FieldValue::U64(n) => i64::try_from(*n).ok(),
            _ => None,
        })
    }

    /// bool field, if present and recorded as a boolean.
    pub fn field_bool(&self, name: &str) -> Option<bool> {
        self.find(name).and_then(|v| match v {
            FieldValue::Bool(b) => Some(*b),
            _ => None,
        })
    }

    /// f64 field, if present and recorded as a float.
    pub fn field_f64(&self, name: &str) -> Option<f64> {
        self.find(name).and_then(|v| match v {
            FieldValue::F64(x) => Some(*x),
            _ => None,
        })
    }

    /// The span's metadata name.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn find(&self, name: &str) -> Option<&FieldValue> {
        self.fields
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

impl CapturedEvent {
    /// String field, if present and recorded as a string.
    pub fn field_str(&self, name: &str) -> Option<String> {
        self.find(name).and_then(|v| match v {
            FieldValue::Str(s) => Some(s.clone()),
            FieldValue::Debug(s) => Some(s.clone()),
            _ => None,
        })
    }

    /// i64 field, if present and recorded as an integer.
    pub fn field_i64(&self, name: &str) -> Option<i64> {
        self.find(name).and_then(|v| match v {
            FieldValue::I64(n) => Some(*n),
            FieldValue::U64(n) => i64::try_from(*n).ok(),
            _ => None,
        })
    }

    /// bool field, if present and recorded as a boolean.
    pub fn field_bool(&self, name: &str) -> Option<bool> {
        self.find(name).and_then(|v| match v {
            FieldValue::Bool(b) => Some(*b),
            _ => None,
        })
    }

    /// f64 field, if present and recorded as a float.
    pub fn field_f64(&self, name: &str) -> Option<f64> {
        self.find(name).and_then(|v| match v {
            FieldValue::F64(x) => Some(*x),
            _ => None,
        })
    }

    /// The event's name — `event_name` field if present, else the metadata name.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn find(&self, name: &str) -> Option<&FieldValue> {
        self.fields
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

#[derive(Debug, Default)]
struct CaptureInner {
    spans: Vec<CapturedSpan>,
    events: Vec<CapturedEvent>,
}

/// Cloneable, Send+Sync handle for querying the captured spans and events.
///
/// Cloning the handle shares the underlying buffer — helpers can clone it
/// and see everything the layer has captured.
#[derive(Debug, Clone, Default)]
pub struct SpanCapture {
    inner: Arc<Mutex<CaptureInner>>,
}

impl SpanCapture {
    /// All captured spans whose metadata name matches exactly.
    pub fn spans_by_name(&self, name: &str) -> Vec<CapturedSpan> {
        self.inner
            .lock()
            .expect("span capture poisoned")
            .spans
            .iter()
            .filter(|s| s.name == name)
            .cloned()
            .collect()
    }

    /// All captured events whose resolved name matches exactly.
    pub fn events_by_name(&self, name: &str) -> Vec<CapturedEvent> {
        self.inner
            .lock()
            .expect("span capture poisoned")
            .events
            .iter()
            .filter(|e| e.name == name)
            .cloned()
            .collect()
    }

    /// All captured spans, in order.
    pub fn spans(&self) -> Vec<CapturedSpan> {
        self.inner
            .lock()
            .expect("span capture poisoned")
            .spans
            .clone()
    }

    /// All captured events, in order.
    pub fn events(&self) -> Vec<CapturedEvent> {
        self.inner
            .lock()
            .expect("span capture poisoned")
            .events
            .clone()
    }
}

/// A `tracing_subscriber::Layer` that captures spans and events into a
/// [`SpanCapture`] handle. Install it on a `Registry`, run the code under
/// test inside `with_default`, and then query the handle.
pub struct SpanCaptureLayer {
    inner: Arc<Mutex<CaptureInner>>,
}

impl SpanCaptureLayer {
    /// Build a new layer + paired query handle.
    ///
    /// The layer is consumed by `Registry::default().with(layer)`; the
    /// handle is kept for post-hoc assertions.
    pub fn new() -> (Self, SpanCapture) {
        let inner = Arc::new(Mutex::new(CaptureInner::default()));
        (
            Self {
                inner: inner.clone(),
            },
            SpanCapture { inner },
        )
    }
}

impl<S: Subscriber> Layer<S> for SpanCaptureLayer {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        attrs.record(&mut visitor);
        self.inner
            .lock()
            .expect("span capture poisoned")
            .spans
            .push(CapturedSpan {
                id: id.into_u64(),
                name: attrs.metadata().name().to_string(),
                fields,
            });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let mut new_fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut new_fields);
        values.record(&mut visitor);
        let span_id = id.into_u64();
        let mut inner = self.inner.lock().expect("span capture poisoned");
        if let Some(span) = inner.spans.iter_mut().find(|s| s.id == span_id) {
            span.fields.extend(new_fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        event.record(&mut visitor);
        // Resolve event name: prefer the `event_name` or `event` field if
        // the caller set one (the sidequest-api convention), else fall back
        // to the tracing metadata name.
        let name = fields
            .iter()
            .find(|(k, _)| k == "event_name" || k == "event")
            .and_then(|(_, v)| match v {
                FieldValue::Str(s) => Some(s.clone()),
                FieldValue::Debug(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| event.metadata().name().to_string());
        self.inner
            .lock()
            .expect("span capture poisoned")
            .events
            .push(CapturedEvent { name, fields });
    }
}

struct FieldCaptureVisitor<'a>(&'a mut Vec<(String, FieldValue)>);

impl<'a> Visit for FieldCaptureVisitor<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0
            .push((field.name().to_string(), FieldValue::I64(value)));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0
            .push((field.name().to_string(), FieldValue::U64(value)));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0
            .push((field.name().to_string(), FieldValue::F64(value)));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0
            .push((field.name().to_string(), FieldValue::Bool(value)));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((
            field.name().to_string(),
            FieldValue::Debug(format!("{value:?}")),
        ));
    }
}
