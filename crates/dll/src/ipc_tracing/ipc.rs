use std::time::SystemTime;

use asdf_overlay_common::event::{
    OverlayEvent,
    tracing::{LogLevel, TracingEvent, TracingMetadata},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span,
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

use crate::event_sink::EventSink;

pub fn layer<S>() -> impl Layer<S>
where
    S: Subscriber + Subscriber + for<'a> LookupSpan<'a>,
{
    IpcTracingLayer
}

struct IpcTracingLayer;

impl<S> Layer<S> for IpcTracingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let metadata = get_metadata(span.metadata(), SystemTime::now());

        EventSink::emit(OverlayEvent::Tracing(TracingEvent::Enter(metadata)));
    }

    fn on_exit(&self, _id: &span::Id, _ctx: Context<'_, S>) {
        EventSink::emit(OverlayEvent::Tracing(TracingEvent::Exit));
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = get_metadata(event.metadata(), SystemTime::now());
        let message = get_message(event);

        EventSink::emit(OverlayEvent::Tracing(TracingEvent::Event {
            metadata,
            message,
        }));
    }
}

fn get_metadata(metadata: &tracing::Metadata, time: SystemTime) -> TracingMetadata {
    TracingMetadata {
        level: to_overlay_log_level(*metadata.level()),
        time,
        module_path: metadata.module_path().map(ToString::to_string),
        line: metadata.line(),
    }
}

fn get_message(event: &Event) -> Option<String> {
    struct MessageVisitor(Option<String>);
    impl Visit for MessageVisitor {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "message" {
                self.0 = Some(value.to_string());
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = Some(format!("{value:?}"));
            }
        }
    }

    let mut visitor = MessageVisitor(None);
    event.record(&mut visitor);
    visitor.0
}

fn to_overlay_log_level(level: tracing::Level) -> LogLevel {
    match level {
        tracing::Level::TRACE => LogLevel::Trace,
        tracing::Level::DEBUG => LogLevel::Debug,
        tracing::Level::INFO => LogLevel::Info,
        tracing::Level::WARN => LogLevel::Warn,
        tracing::Level::ERROR => LogLevel::Error,
    }
}
