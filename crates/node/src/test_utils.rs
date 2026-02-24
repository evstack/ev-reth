//! test utilities for verifying tracing instrumentation.

use std::sync::{Arc, Mutex};
use tracing::{
    field::{Field, Visit},
    subscriber::set_default,
    Subscriber,
};
use tracing_subscriber::{
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    Layer,
};

/// a recorded span with its name and captured fields.
#[derive(Debug, Clone)]
pub(crate) struct SpanRecord {
    pub(crate) name: String,
    pub(crate) fields: Vec<(String, String)>,
}

impl SpanRecord {
    pub(crate) fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|(k, _)| k == name)
    }
}

/// collects field values from span attributes.
struct FieldCollector {
    fields: Vec<(String, String)>,
}

impl FieldCollector {
    fn new() -> Self {
        Self { fields: Vec::new() }
    }
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

/// a tracing layer that records span metadata for test assertions.
#[derive(Debug, Clone)]
pub(crate) struct SpanCollector {
    spans: Arc<Mutex<Vec<SpanRecord>>>,
}

impl SpanCollector {
    pub(crate) fn new() -> Self {
        Self {
            spans: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// finds the first span with the given name.
    pub(crate) fn find_span(&self, name: &str) -> Option<SpanRecord> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.name == name)
            .cloned()
    }

    /// installs this collector as the default subscriber for the current thread,
    /// returning a guard that restores the previous subscriber on drop.
    pub(crate) fn as_default(&self) -> tracing::subscriber::DefaultGuard {
        let subscriber = tracing_subscriber::registry().with(self.clone());
        set_default(subscriber)
    }
}

impl<S> Layer<S> for SpanCollector
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut collector = FieldCollector::new();
        attrs.record(&mut collector);

        let record = SpanRecord {
            name: attrs.metadata().name().to_string(),
            fields: collector.fields,
        };

        self.spans.lock().unwrap().push(record);
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: Context<'_, S>,
    ) {
        let mut collector = FieldCollector::new();
        values.record(&mut collector);

        if let Some(span_ref) = ctx.span(id) {
            let name = span_ref.name().to_string();
            let mut spans = self.spans.lock().unwrap();
            if let Some(record) = spans.iter_mut().find(|s| s.name == name) {
                record.fields.extend(collector.fields);
            }
        }
    }
}
