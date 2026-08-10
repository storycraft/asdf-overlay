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

impl From<asdf_overlay_common::surface::GpuLuid> for GpuLuid {
    fn from(val: asdf_overlay_common::surface::GpuLuid) -> Self {
        Self {
            low: val.low,
            high: val.high,
        }
    }
}

impl From<GpuLuid> for asdf_overlay_common::surface::GpuLuid {
    fn from(val: GpuLuid) -> Self {
        asdf_overlay_common::surface::GpuLuid {
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

/// Utility function to create `PercentLength` using percent relative value.
#[napi]
pub fn percent(value: f64) -> PercentLength {
    PercentLength::Percent { value }
}

/// Utility function to create `PercentLength` using absolute length value.
#[napi]
pub fn length(value: f64) -> PercentLength {
    PercentLength::Length { value }
}
