//! ## Asdf Overlay
//! Asdf overlay let you put overlay infront of existing windows gpu framebuffer.
//!
//! It hooks various graphics API call to detect graphical windows in the process.
//! Asdf overlay automatically decides which graphics API the window is using,
//! chooses suitable renderer.
//!
//! It can also capture inputs going through the target window.
//! You can listen them or even block them from reaching application handlers.
//!
//! ## Example
//! ```no_run
//! use asdf_overlay::initialize;
//! use asdf_overlay::event_sink::OverlayEventSink;
//!
//! let module_handle, window_hwnd;
//! // Initialize asdf-overlay.
//! initialize(module_handle).expect("initialization failed");
//!
//! // Initialize Event sink.
//! // Without setting it, the overlay will not render.
//! // This is intended because windows state will be out of sync if you miss any events.
//! OverlayEventSink::set(move |event| {
//!     // Do something with events.
//! });
//!
//! Backends::with_backend(window_hwnd, |backend| {
//!     // Do something with overlay window backend.
//! });
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

pub mod backend;
pub mod event_sink;
pub mod interop;
pub mod layout;
pub mod surface;

mod hook;
mod renderer;
mod resources;
mod texture;
mod types;
mod util;

use anyhow::{Context, bail};
use once_cell::sync::OnceCell;
use windows::Win32::Foundation::HINSTANCE;

/// Module handle of the overlay.
static INSTANCE: OnceCell<usize> = OnceCell::new();

#[inline]
/// Get overlay [`HINSTANCE`]
pub(crate) fn instance() -> HINSTANCE {
    HINSTANCE(*INSTANCE.get().unwrap() as _)
}

/// Initialize overlay, hooks.
///
/// * Calling more than once will fail.
/// * Calling with holding loader lock (DllMain) will fail.
/// * If given `hinstance` is invalid, some resources may not appear correctly.
pub fn initialize(hinstance: usize) -> anyhow::Result<()> {
    if INSTANCE.set(hinstance).is_err() {
        bail!("Already initialized");
    }

    hook::install(HINSTANCE(hinstance as _)).context("hook initialization failed")?;
    Ok(())
}
