use asdf_overlay_client::common;
use napi_derive::napi;

#[napi(object)]
pub struct SurfaceInfo {
    /// Graphics API used by the surface.
    pub api: RenderApi,

    /// Window id of the surface, if any.
    pub window_id: Option<u32>,

    /// GPU LUID of the surface.
    pub gpu_id: GpuLuid,
}

impl From<common::event::surface::SurfaceInfo> for SurfaceInfo {
    fn from(v: common::event::surface::SurfaceInfo) -> Self {
        Self {
            api: RenderApi::from(v.api),
            window_id: v.window_id,
            gpu_id: GpuLuid::from(v.gpu_id),
        }
    }
}

#[napi(string_enum)]
pub enum RenderApi {
    Opengl,
    Direct3D9,
    Direct3D11,
    Direct3D12,
    Vulkan,
}

impl From<common::event::surface::RenderApi> for RenderApi {
    fn from(v: common::event::surface::RenderApi) -> Self {
        match v {
            common::event::surface::RenderApi::Opengl => Self::Opengl,
            common::event::surface::RenderApi::Direct3D9 => Self::Direct3D9,
            common::event::surface::RenderApi::Direct3D11 => Self::Direct3D11,
            common::event::surface::RenderApi::Direct3D12 => Self::Direct3D12,
            common::event::surface::RenderApi::Vulkan => Self::Vulkan,
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
