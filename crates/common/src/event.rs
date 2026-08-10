//! The [`OverlayEvent`] enum and assorted types.
//!
//! These events are emitted from overlay system and usually sent from server to client via IPC connection.
//! For the actual usage inside the library, see the documentation of
//! * Overlay system: `asdf-overlay`
//! * IPC client: `asdf-overlay-client`
//! * IPC server: `asdf-overlay-dll`
use asdf_overlay_window_event::WindowEvent;
use bitcode::{Decode, Encode};

use crate::surface::SurfaceEvent;

pub use asdf_overlay_window_event as window;

/// Describe a overlay event.
#[derive(Debug, Clone, Encode, Decode)]
pub enum OverlayEvent {
    /// Events related to a specific window.
    Window { id: u32, event: WindowEvent },

    /// Events related to a specific surface.
    Surface { id: u32, event: SurfaceEvent },

    /// Input blocking is turned off or interrupted by the user or system.
    ///
    /// The user may turn off input blocking at any time,
    /// for example, by pressing Alt+F4 on Windows.
    InputBlockingEnded,

    /// Log message from overlay system.
    Log {},
}
