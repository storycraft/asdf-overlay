use egui::RequestRepaintInfo;

#[derive(derive_more::From)]
pub enum Event {
    Overlay(#[from] asdf_overlay_event::Event),
    Window(#[from] asdf_overlay_window_event::Event),
    RequestRepaint(#[from] RequestRepaintInfo),
}
