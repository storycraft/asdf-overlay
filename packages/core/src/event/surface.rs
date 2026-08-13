use asdf_overlay_client::common;
use napi_derive::napi;

#[napi(object)]
pub struct SurfaceInfo {
    /// Surface type.
    pub ty: SurfaceType,

    /// GPU LUID of the surface.
    pub gpu_id: GpuLuid,
}

impl From<common::event::surface::SurfaceInfo> for SurfaceInfo {
    fn from(v: common::event::surface::SurfaceInfo) -> Self {
        Self {
            ty: SurfaceType::from(v.api),
            gpu_id: GpuLuid::from(v.gpu_id),
        }
    }
}

#[napi]
pub enum SurfaceType {
    Opengl { window_id: u32 },
    Direct3D9 { window_id: u32 },
    Direct3D11 { window_id: Option<u32> },
    Direct3D12 { window_id: Option<u32> },
    Vulkan { window_id: u32 },
}

impl From<common::event::surface::SurfaceType> for SurfaceType {
    fn from(v: common::event::surface::SurfaceType) -> Self {
        match v {
            common::event::surface::SurfaceType::Opengl { window_id } => Self::Opengl { window_id },
            common::event::surface::SurfaceType::Direct3D9 { window_id } => {
                Self::Direct3D9 { window_id }
            }
            common::event::surface::SurfaceType::Direct3D11 { window_id } => {
                Self::Direct3D11 { window_id }
            }
            common::event::surface::SurfaceType::Direct3D12 { window_id } => {
                Self::Direct3D12 { window_id }
            }
            common::event::surface::SurfaceType::Vulkan { window_id } => Self::Vulkan { window_id },
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
