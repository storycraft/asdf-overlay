pub mod event;
pub mod overlay;
pub mod surface;

use asdf_overlay_client::common::size;
use mimalloc::MiMalloc;
use napi_derive::napi;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[napi(object)]
pub struct GpuLuid {
    pub low: u32,
    pub high: i32,
}

impl From<asdf_overlay_event::GpuLuid> for GpuLuid {
    fn from(val: asdf_overlay_event::GpuLuid) -> Self {
        Self {
            low: val.low,
            high: val.high,
        }
    }
}

impl From<GpuLuid> for asdf_overlay_event::GpuLuid {
    fn from(val: GpuLuid) -> Self {
        asdf_overlay_event::GpuLuid {
            low: val.low,
            high: val.high,
        }
    }
}

#[napi]
pub enum PercentLength {
    Percent { value: f64 },
    Length { value: f64 },
}

impl From<PercentLength> for size::PercentLength {
    fn from(val: PercentLength) -> Self {
        match val {
            PercentLength::Percent { value } => size::PercentLength::Percent(value as _),
            PercentLength::Length { value } => size::PercentLength::Length(value as _),
        }
    }
}
