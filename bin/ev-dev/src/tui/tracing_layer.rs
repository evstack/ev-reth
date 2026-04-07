use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{
    field::{Field, Visit},
    Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::app::LogEntry;

struct FieldCollector {
    fields: Vec<(String, String)>,
}

impl FieldCollector {
    const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    fn take_message(&mut self) -> String {
        if let Some(pos) = self.fields.iter().position(|(k, _)| k == "message") {
            self.fields.remove(pos).1
        } else {
            String::new()
        }
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

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

pub(crate) struct TuiTracingLayer {
    tx: mpsc::Sender<LogEntry>,
}

impl TuiTracingLayer {
    pub(crate) const fn new(tx: mpsc::Sender<LogEntry>) -> Self {
        Self { tx }
    }
}

impl<S> Layer<S> for TuiTracingLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut collector = FieldCollector::new();
        event.record(&mut collector);

        let message = collector.take_message();
        let metadata = event.metadata();

        let entry = LogEntry {
            level: *metadata.level(),
            target: metadata.target().to_string(),
            message,
            fields: collector.fields,
            timestamp: Instant::now(),
        };

        let _ = self.tx.try_send(entry);
    }
}
