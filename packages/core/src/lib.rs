pub mod event;
pub mod overlay;
pub mod surface;

use asdf_overlay_client::common;
use mimalloc::MiMalloc;
use napi_derive::napi;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[napi]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<common::event::LogLevel> for LogLevel {
    fn from(value: common::event::LogLevel) -> Self {
        match value {
            common::event::LogLevel::Trace => Self::Trace,
            common::event::LogLevel::Debug => Self::Debug,
            common::event::LogLevel::Info => Self::Info,
            common::event::LogLevel::Warn => Self::Warn,
            common::event::LogLevel::Error => Self::Error,
        }
    }
}

#[napi(object)]
pub struct GpuLuid {
    pub low: u32,
    pub high: i32,
}

impl From<common::event::surface::GpuLuid> for GpuLuid {
    fn from(val: common::event::surface::GpuLuid) -> Self {
        Self {
            low: val.low,
            high: val.high,
        }
    }
}

impl From<GpuLuid> for common::event::surface::GpuLuid {
    fn from(val: GpuLuid) -> Self {
        common::event::surface::GpuLuid {
            low: val.low,
            high: val.high,
        }
    }
}
