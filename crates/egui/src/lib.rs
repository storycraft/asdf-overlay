#[cfg(feature = "dll")]
pub mod dll;
mod event;
pub mod prelude;
pub mod runner;
mod state;

use std::sync::Arc;

use asdf_overlay_window::Backends;
use egui::{Context, Ui, Visuals};

pub trait App {
    fn ui(&mut self, ui: &mut Ui, overlay_cx: &OverlayContext);

    fn logic(&mut self, _cx: &Context, _overlay_cx: &OverlayContext) {}

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
}

impl OverlayContext {
    pub fn block_input(&self) {
        self.windows.block_input();
    }

    pub fn unblock_input(&self) {
        self.windows.unblock_input();
    }
}
