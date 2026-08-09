use std::{sync::Arc, thread};

use asdf_overlay_window::Backends;
use eframe::egui;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

// egui example from https://github.com/emilk/egui/blob/main/examples/hello_world/src/main.rs

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let backends = Arc::new(Backends::new(unsafe { GetModuleHandleW(None) }?.0 as _)?);
    thread::spawn({
        let backends = backends.clone();
        move || {
            while let Some(event) = backends.recv() {
                eprintln!("Backend event: {event:?}");
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native(
        "My egui App",
        options,
        Box::new(|_| {
            Ok(Box::new(MyApp {
                backends,
                name: "Arthur".to_owned(),
                age: 42,
            }))
        }),
    )?;
    Ok(())
}

struct MyApp {
    backends: Arc<Backends>,
    name: String,
    age: u32,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            if ui.button("Block input").clicked() {
                self.backends.block_input();
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
}
