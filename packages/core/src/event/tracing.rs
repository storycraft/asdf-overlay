use std::time::SystemTime;

use asdf_overlay_client::common;
use napi_derive::napi;

#[napi(object)]
pub struct TracingMetadata {
    /// Metadata tracing level.
    pub level: LogLevel,

    /// The time in milliseconds when the tracing event occurred.
    pub time: f64,

    /// The module path of the tracing event, if available.
    pub module_path: Option<String>,

    /// The line number of the tracing event, if available.
    pub line: Option<u32>,
}

impl From<common::event::tracing::TracingMetadata> for TracingMetadata {
    fn from(value: common::event::tracing::TracingMetadata) -> Self {
        Self {
            level: LogLevel::from(value.level),
            time: value
                .time
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as f64,
            module_path: value.module_path,
            line: value.line,
        }
    }
}

#[napi(string_enum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<common::event::tracing::LogLevel> for LogLevel {
    fn from(value: common::event::tracing::LogLevel) -> Self {
        match value {
            common::event::tracing::LogLevel::Trace => Self::Trace,
            common::event::tracing::LogLevel::Debug => Self::Debug,
            common::event::tracing::LogLevel::Info => Self::Info,
            common::event::tracing::LogLevel::Warn => Self::Warn,
            common::event::tracing::LogLevel::Error => Self::Error,
        }
    }
}
