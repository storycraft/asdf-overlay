//! Render overlays into graphics surfaces in the current Windows process.
//!
//! Graphics hooks discover surfaces as the application presents them. Install an
//! event sink to receive lifecycle events and enable rendering. Input interception
//! is provided separately by `asdf-overlay-window`.
//!
//! # Example
//! ```no_run
//! use asdf_overlay::{event_sink::OverlayEventSink, initialize, surface::Surfaces};
//!
//! // Run outside DllMain / the Windows loader lock.
//! OverlayEventSink::set(|event| {
//!     // Queue events for your application to process.
//! });
//! initialize().expect("initialization failed");
//!
//! for id in Surfaces::iter() {
//!     Surfaces::state(id, |state| state.reposition(16, 16));
//! }
//! ```

#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
/// Generated OpenGL bindings and global function tables.
mod gl {
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
/// Generated WGL bindings and global function tables.
mod wgl {
    include!(concat!(env!("OUT_DIR"), "/wgl_bindings.rs"));
}

pub mod event_sink;
pub mod surface;

mod hook;
pub mod interop;
mod renderer;
mod types;
mod util;

use anyhow::Context;

/// Install graphics hooks used to discover and render overlay surfaces.
///
/// Call outside `DllMain` and the Windows loader lock: setup creates a temporary
/// window and graphics resources and may block. Install an
/// [`event_sink::OverlayEventSink`] to enable surface detection and rendering.
/// This does not initialize the separate window/input backend.
///
/// Returns an error if temporary-window setup fails. Individual graphics-hook
/// failures may instead be logged, so success does not guarantee every graphics
/// API is supported. Hooks are process-wide and are not undone on return or error;
/// this is not a restart or teardown API.
pub fn initialize() -> anyhow::Result<()> {
    hook::install().context("hook initialization failed")?;
    Ok(())
}
