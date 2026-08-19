#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u64);

#[cfg(feature = "napi")]
const _: () = {
    use napi::bindgen_prelude::*;

    impl ToNapiValue for SurfaceId {
        unsafe fn to_napi_value(env: sys::napi_env, val: Self) -> Result<sys::napi_value> {
            unsafe { <u64>::to_napi_value(env, val.0) }
        }
    }

    impl FromNapiValue for SurfaceId {
        unsafe fn from_napi_value(env: sys::napi_env, napi_val: sys::napi_value) -> Result<Self> {
            let (_, v, _) = unsafe { <BigInt>::from_napi_value(env, napi_val)? }.get_u64();
            Ok(SurfaceId(v))
        }
    }
};

#[cfg(feature = "uniffi")]
uniffi::custom_type!(SurfaceId, u64);

impl From<u64> for SurfaceId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<SurfaceId> for u64 {
    fn from(id: SurfaceId) -> Self {
        id.0
    }
}

#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OverlayEvent {
    /// Events related to a specific surface.
    Surface {
        /// Unique identifier for the surface.
        id: SurfaceId,
        event: SurfaceEvent,
    },
}

#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SurfaceEvent {
    Added {
        /// Width of the surface
        width: u32,

        /// Height of the surface
        height: u32,

        /// Surface information
        info: SurfaceInfo,
    },
    Resized {
        // New width of the surface
        width: u32,

        // New height of the surface
        height: u32,
    },
    Destroyed,
}

#[cfg_attr(feature = "napi", napi_derive::napi(object))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
