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
mod window;

use std::sync::Arc;

use asdf_overlay_event::SurfaceInfo;
use asdf_overlay_window::Backends;
use egui::{Context, Ui, Visuals};

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

    /// Handle the end of input blocking before the next repaint. Defaults to no action.
    fn on_input_blocking_ended(&mut self) {}

    /// Return the RGBA clear color, with components in `0.0..=1.0`.
    ///
    /// Defaults to transparent black.
    fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[non_exhaustive]
pub struct CreationContext {
    pub egui_cx: Context,
}

/// Controls host input and exposes the selected overlay surface.
///
/// Input blocking applies to all intercepted windows in the process. Cursor and
/// IME changes may complete after the control methods return.
#[non_exhaustive]
pub struct OverlayContext {
    pub(crate) windows: Arc<Backends>,
    pub(crate) info: SurfaceInfo,
}

impl OverlayContext {
    /// Block host input, doing nothing if already blocked.
    pub fn block_input(&self) {
        self.windows.block_input();
    }

    /// Unblock host input.
    pub fn unblock_input(&self) {
        self.windows.unblock_input();
    }

    /// Return metadata for the selected surface.
    pub fn surface_info(&self) -> &SurfaceInfo {
        &self.info
    }
}
