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
    /// Surface type and type specific informations.
    pub api: SurfaceType,

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
pub enum SurfaceType {
    /// Surface is OpenGL default framebuffer.
    Opengl {
        /// Window id of the OpenGL surface
        window_id: u32,
    },

    /// Surface is Direct3D9 swapchain.
    Direct3D9 {
        /// Window id of the Direct3D9 surface
        window_id: u32,
    },

    /// Surface is Direct3D11 swapchain.
    Direct3D11 {
        /// Window id of the Direct3D11 surface
        ///
        /// If the surface is directcomposition swapchain, the window id will be None.
        window_id: Option<u32>,
    },

    /// Surface is Direct3D12 swapchain.
    Direct3D12 {
        /// Window id of the Direct3D12 surface
        ///
        /// If the surface is directcomposition swapchain, the window id will be None.
        window_id: Option<u32>,
    },

    /// Surface is Vulkan win32 surface.
    Vulkan {
        /// Window id of the Vulkan surface.
        window_id: u32,
    },
}
