//! The [`Event`] enum and assorted types.

pub mod input;

use input::InputEvent;

/// Describe a event.
#[derive(Debug, Clone)]
pub enum Event {
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
