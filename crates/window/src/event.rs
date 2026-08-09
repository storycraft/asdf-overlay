//! The [`OverlayEvent`] enum and assorted types.
//!
//! These events are emitted from overlay system and usually sent from server to client via IPC connection.
//! For the actual usage inside the library, see the documentation of
//! * Overlay system: `asdf-overlay`
//! * IPC client: `asdf-overlay-client`
//! * IPC server: `asdf-overlay-dll`

pub mod input;

use input::InputEvent;

/// Describe a backend event.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// Events related to a specific window.
    Window {
        /// Unique identifier for the window.
        id: u32,
        event: WindowEvent,
    },

    /// Input blocking is turned off or interrupted by the user or system.
    ///
    /// The user may turn off input blocking at any time,
    /// for example, by pressing Alt+F4 on Windows.
    InputBlockingEnded,
}

/// Describe a window event.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// A new window is identified.
    Added {
        /// Initial width of the window
        width: u32,

        /// Initial height of the window
        height: u32,
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

    /// Window is no longer available.
    /// This is likely the last event for this window.
    Destroyed,
}
