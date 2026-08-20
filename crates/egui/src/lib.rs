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

pub trait App {
    fn ui(&mut self, ui: &mut Ui, overlay_cx: &OverlayContext);

    fn logic(&mut self, _cx: &Context, _overlay_cx: &OverlayContext) {}

    fn on_input_blocking_ended(&mut self) {}

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
    pub fn block_input(&self) {
        self.windows.block_input();
    }

    pub fn unblock_input(&self) {
        self.windows.unblock_input();
    }

    pub fn surface_info(&self) -> &SurfaceInfo {
        &self.surface.info
    }
}
