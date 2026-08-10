use bitcode::{Decode, Encode};

/// Describes a surface event.
#[derive(Debug, Clone, Encode, Decode)]
pub enum SurfaceEvent {
    Added,
    Resized { width: u32, height: u32 },
    Destroyed,
}

/// Locally unique identifier for a GPU adapter.
///
/// This identifier is not persistent across reboots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct GpuLuid {
    /// The low part of the LUID.
    pub low: u32,
    /// The high part of the LUID.
    pub high: i32,
}
