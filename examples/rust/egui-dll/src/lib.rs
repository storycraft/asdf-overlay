use core::error::Error;

use asdf_overlay_egui::prelude::*;

fn main() -> Result<(), Box<dyn Error>> {
    run_app(async |_| {
        Ok(MyApp {
            name: "Arthur".to_owned(),
            age: 42,
        })
    })
}
impl_dll!(main);

struct MyApp {
    name: String,
    age: u32,
}

impl App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.input(|input| input.clone()).ui(ui);

        egui::Window::new("Egui window")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size((320.0, 180.0))
            .show(ui, |ui| {
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
}
