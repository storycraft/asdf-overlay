#[cfg(feature = "dll")]
pub mod dll;
mod event;
pub mod prelude;
pub mod runner;
mod state;

use egui::{Context, Ui, Visuals};

pub trait App {
    fn ui(&mut self, ui: &mut Ui);

    fn logic(&mut self, _ctx: &Context) {}

    fn clear_color(&self, _visuals: &Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

#[non_exhaustive]
pub struct CreationContext {
    pub egui_ctx: Context,
}
