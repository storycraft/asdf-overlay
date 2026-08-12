/// Describe a event.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Event {
    /// Events related to a specific surface.
    Surface {
        /// Unique identifier for the surface.
        id: u64,
        event: SurfaceEvent,
    },
}

/// Describes a surface event.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Hint for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SurfaceInfo {
    /// The render api used by the surface.
    pub api: RenderApi,

    /// The window id of the surface, if any.
    pub window_id: Option<u32>,

    /// The LUID of the GPU adapter which the window used to present to surface.
    ///
    /// Client must choose correct GPU adapter using this luid,
    /// otherwise overlay rendering may fail.
    pub gpu_id: GpuLuid,
}

/// Locally unique identifier for a GPU adapter.
///
/// This identifier is not persistent across reboots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GpuLuid {
    /// The low part of the LUID.
    pub low: u32,
    /// The high part of the LUID.
    pub high: i32,
}

/// Describes the render api used by a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RenderApi {
    Opengl,
    Direct3D9,
    Direct3D11,
    Direct3D12,
    Vulkan,
}
