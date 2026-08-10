pub mod event;
pub mod overlay;
pub mod surface;

use asdf_overlay_client::common;
use mimalloc::MiMalloc;
use napi_derive::napi;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

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
