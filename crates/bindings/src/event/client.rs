pub mod tracing;

use crate::event::{
    client::tracing::TracingEvent,
    surface::{SurfaceEvent, SurfaceId},
    window::WindowEvent,
};

#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ClientEvent {
    /// Events related to a specific window.
    Window { id: u32, event: WindowEvent },

    /// Events related to a specific surface.
    Surface { id: SurfaceId, event: SurfaceEvent },

    /// Input blocking is turned off or interrupted by the user or system.
    ///
    /// The user may turn off input blocking at any time,
    /// for example, by pressing Alt+F4 on Windows.
    InputBlockingEnded,

    /// A tracing from overlay system.
    Tracing(TracingEvent),
}
