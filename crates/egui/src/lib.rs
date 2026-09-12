//! Run an egui application over intercepted graphics surfaces in the host process.
//!
//! [`runner::run_app`] owns the global input and rendering setup. Use one runner
//! per process and keep application callbacks responsive to avoid delaying events.

#[cfg(feature = "dll")]
pub mod dll;
mod event;
pub mod prelude;
pub mod runner;
mod state;

use std::sync::Arc;

use asdf_overlay_event::SurfaceInfo;
use asdf_overlay_window::Backends;
use egui::{Context, Ui, Visuals};

use crate::state::SurfaceState;

/// Application callbacks invoked by the overlay runner.
pub trait App {
    /// Build the UI for an egui pass on the selected overlay surface.
    ///
    /// egui may run multiple passes for a repaint; avoid assuming one invocation
    /// per displayed frame when performing side effects.
    fn ui(&mut self, ui: &mut Ui, overlay_cx: &OverlayContext);

    /// Update application logic before processing a repaint, outside the UI pass.
    ///
    /// The default does nothing. This is repaint-driven, not a fixed-rate tick.
    fn logic(&mut self, _cx: &Context, _overlay_cx: &OverlayContext) {}

    /// React to a process-wide input-blocking-ended event; default does nothing.
    ///
    /// The runner requests a repaint afterward. Thread-specific restoration may
    /// still be queued when this callback runs.
    fn on_input_blocking_ended(&mut self) {}

    /// Return the RGBA clear color used before rendering the overlay.
    ///
    /// The default is transparent black. Components are passed to the renderer
    /// unchanged; use finite normalized values in `0.0..=1.0`.
    fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[non_exhaustive]
pub struct CreationContext {
    pub egui_cx: Context,
}

#[non_exhaustive]
pub struct OverlayContext {
    pub(crate) windows: Arc<Backends>,
    pub(crate) surface: SurfaceState,
}

impl OverlayContext {
    /// Block host input across intercepted windows, including those of other surfaces.
    ///
    /// Thread-specific cursor/IME changes are queued; repeated blocking is a no-op.
    pub fn block_input(&self) {
        self.windows.block_input();
    }

    /// End process-wide blocking; queued cursor/IME restoration may complete later.
    pub fn unblock_input(&self) {
        self.windows.unblock_input();
    }

    /// Borrow metadata for the currently selected surface and its GPU.
    ///
    /// Composition surfaces can have no window ID; do not assume one is present.
    pub fn surface_info(&self) -> &SurfaceInfo {
        &self.surface.info
    }
}
