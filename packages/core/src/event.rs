pub mod input;
pub mod ime;

use napi_derive::napi;

use crate::{GpuLuid, event::input::InputEvent};

#[napi]
pub enum OverlayEvent {
    /// Events related to a specific window.
    Window {
        /// Unique identifier for the window.
        id: u32,
        event: WindowEvent,
    },
}

impl From<asdf_overlay_event::OverlayEvent> for OverlayEvent {
    fn from(event: asdf_overlay_event::OverlayEvent) -> Self {
        match event {
            asdf_overlay_event::OverlayEvent::Window { id, event } => Self::Window {
                id,
                event: event.into(),
            },
        }
    }
}

/// Describe a window event.
#[napi]
pub enum WindowEvent {
    /// A new window capable for overlay rendering is identified.
    Added {
        /// Initial width of the window.
        width: u32,

        /// Initial height of the window.
        height: u32,

        /// The LUID of the GPU adapter which the window used to present to surface.
        ///
        /// Client must choose correct GPU adapter using this luid,
        /// otherwise overlay rendering may fail.
        gpu_id: GpuLuid,
    },

    /// Window size is changed.
    Resized {
        /// New width of the window.
        width: u32,

        /// New height of the window.
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

impl From<asdf_overlay_event::WindowEvent> for WindowEvent {
    fn from(event: asdf_overlay_event::WindowEvent) -> Self {
        match event {
            asdf_overlay_event::WindowEvent::Added {
                width,
                height,
                gpu_id,
            } => Self::Added {
                width,
                height,
                gpu_id: gpu_id.into(),
            },
            asdf_overlay_event::WindowEvent::Resized { width, height } => {
                Self::Resized { width, height }
            }
            asdf_overlay_event::WindowEvent::Input(input) => Self::Input(input.into()),
            asdf_overlay_event::WindowEvent::InputBlockingEnded => Self::InputBlockingEnded,
            asdf_overlay_event::WindowEvent::Destroyed => Self::Destroyed,
        }
    }
}
