//! The [`OverlayEvent`] enum and assorted types.
//!
//! These events are emitted from overlay system and usually sent from server to client via IPC connection.
//! For the actual usage inside the library, see the documentation of
//! * Overlay system: `asdf-overlay`
//! * IPC client: `asdf-overlay-client`
//! * IPC server: `asdf-overlay-dll`

pub mod input;

use input::InputEvent;

/// Describe a overlay event.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum OverlayEvent {
    /// Events related to a specific window.
    Window {
        /// Unique identifier for the window.
        id: u32,
        event: WindowEvent,
    },
}

/// Describe a window event.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum WindowEvent {
    /// A new window capable for overlay rendering is identified.
    Added {
        /// Initial width of the window
        width: u32,

        /// Initial height of the window
        height: u32,

        /// The LUID of the GPU adapter which the window used to present to surface.
        ///
        /// Client must choose correct GPU adapter using this luid,
        /// otherwise overlay rendering may fail.
        gpu_id: GpuLuid,
    },

    /// Window size is changed.
    Resized {
        /// New width of the window
        width: u32,

        /// New height of the window
        height: u32,
    },

    /// Input event related to this window.
    ///
    /// You only receive this event if you are listening to input events
    /// or have input blocking enabled for this window.
    Input(InputEvent),

    /// Input blocking is turned off or interrupted by the user or system.
    ///
    /// The user may turn off input blocking at any time,
    /// for example, by pressing Alt+F4 on Windows.
    InputBlockingEnded,

    /// Window is no longer available for overlay rendering.
    /// This is likely the last event for this window.
    Destroyed,
}

/// Locally unique identifier for a GPU adapter.
///
/// This identifier is not persistent across reboots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct GpuLuid {
    /// The low part of the LUID.
    pub low: u32,
    /// The high part of the LUID.
    pub high: i32,
}
