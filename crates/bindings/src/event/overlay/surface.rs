#[cfg_attr(feature = "napi", napi_derive::napi(object))]
pub struct SurfaceInfo {
    /// Surface type.
    pub ty: SurfaceType,

    /// GPU LUID of the surface.
    pub gpu_id: GpuLuid,
}

impl From<asdf_overlay_event::SurfaceInfo> for SurfaceInfo {
    fn from(v: asdf_overlay_event::SurfaceInfo) -> Self {
        Self {
            ty: SurfaceType::from(v.api),
            gpu_id: GpuLuid::from(v.gpu_id),
        }
    }
}

#[cfg_attr(feature = "napi", napi_derive::napi)]
pub enum SurfaceType {
    Opengl { window_id: u32 },
    Direct3D9 { window_id: u32 },
    Direct3D11 { window_id: Option<u32> },
    Direct3D12 { window_id: Option<u32> },
    Vulkan { window_id: u32 },
}

impl From<asdf_overlay_event::SurfaceType> for SurfaceType {
    fn from(v: asdf_overlay_event::SurfaceType) -> Self {
        match v {
            asdf_overlay_event::SurfaceType::Opengl { window_id } => Self::Opengl { window_id },
            asdf_overlay_event::SurfaceType::Direct3D9 { window_id } => {
                Self::Direct3D9 { window_id }
            }
            asdf_overlay_event::SurfaceType::Direct3D11 { window_id } => {
                Self::Direct3D11 { window_id }
            }
            asdf_overlay_event::SurfaceType::Direct3D12 { window_id } => {
                Self::Direct3D12 { window_id }
            }
            asdf_overlay_event::SurfaceType::Vulkan { window_id } => Self::Vulkan { window_id },
        }
    }
}

#[cfg_attr(feature = "napi", napi_derive::napi(object))]
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
