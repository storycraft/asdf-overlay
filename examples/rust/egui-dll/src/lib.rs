use core::error::Error;

use asdf_overlay_egui::{OverlayContext, prelude::*};

#[pollster::main]
async fn main() -> Result<(), Box<dyn Error>> {
    run_app(async |_| {
        Ok(MyApp {
            input_block: false,
            name: "Arthur".to_owned(),
            age: 42,
        })
    })
    .await
}
impl_dll!(main);

struct MyApp {
    input_block: bool,
    name: String,
    age: u32,
}

impl App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, overlay_cx: &OverlayContext) {
        egui::Window::new("Inputs").show(ui, |ui| ui.input(|input| input.clone()).ui(ui));

        egui::Window::new("Egui window")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size((320.0, 180.0))
            .show(ui, |ui| {
                if ui.checkbox(&mut self.input_block, "Block Input").clicked() {
                    if self.input_block {
                        overlay_cx.block_input();
                    } else {
                        overlay_cx.unblock_input();
                    }
                }

                ui.heading("My egui Application");
                ui.horizontal(|ui| {
                    let name_label = ui.label("Your name: ");
                    ui.text_edit_singleline(&mut self.name)
                        .labelled_by(name_label.id);
                });
                ui.add(egui::Slider::new(&mut self.age, 0..=120).text("age"));
                if ui.button("Increment").clicked() {
                    self.age += 1;
                }
                ui.label(format!("Hello '{}', age {}", self.name, self.age));
            });
    }

    fn on_input_blocking_ended(&mut self) {
        self.input_block = false;
    }
}
