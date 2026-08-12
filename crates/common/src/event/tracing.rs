use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingEvent {
    Enter(TracingMetadata),
    Event {
        metadata: TracingMetadata,

        /// The tracing message.
        message: Option<String>,
    },
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingMetadata {
    /// The tracing level.
    pub level: LogLevel,

    /// The time when the metadata was emitted.
    pub time: SystemTime,

    /// The module path of the metadata, if available.
    pub module_path: Option<String>,

    /// The line number of the metadata, if available.
    pub line: Option<u32>,
}

/// Describe a log level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
