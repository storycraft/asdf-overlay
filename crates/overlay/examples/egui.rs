use core::error::Error;

use asdf_overlay::{
    event_sink::OverlayEventSink,
    surface::{SharedTextureHandle, Surfaces},
};
use eframe::{Renderer, egui};
use windows::{
    Win32::Graphics::{
        Direct3D11::{
            D3D11_BIND_SHADER_RESOURCE, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
            D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11Texture2D,
        },
        Dxgi::{
            Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
            DXGI_SHARED_RESOURCE_READ, IDXGIResource1,
        },
    },
    core::Interface,
};

// egui example from https://github.com/emilk/egui/blob/main/examples/hello_world/src/main.rs

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    asdf_overlay::initialize().expect("Overlay initialization");

    OverlayEventSink::set(move |event| {
        eprintln!("Backend event: {event:?}");
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        renderer: Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "My egui App",
        options,
        Box::new(|_| {
            Ok(Box::new(MyApp {
                render_overlay: false,
                name: "Arthur".to_owned(),
                age: 42,
            }))
        }),
    )?;
    Ok(())
}

fn show_overlay() -> anyhow::Result<()> {
    // Create a shared white overlay texture
    fn create_overlay_texture(device: &ID3D11Device) -> anyhow::Result<ID3D11Texture2D> {
        let tex = {
            let mut slot = None;
            unsafe {
                device.CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: 128,
                        Height: 128,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as _,
                        CPUAccessFlags: 0,
                        MiscFlags: D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 as u32
                            | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
                    },
                    Some(&D3D11_SUBRESOURCE_DATA {
                        pSysMem: [0xff000000u32; 128 * 128].as_ptr().cast(),
                        SysMemPitch: 128 * 4,
                        SysMemSlicePitch: 0,
                    }),
                    Some(&mut slot),
                )?;
            }

            slot.unwrap()
        };

        Ok(tex)
    }

    for id in Surfaces::iter() {
        let Some(res) = Surfaces::state(id, |state| {
            let texture = create_overlay_texture(&state.interop.device)?;

            let handle = unsafe {
                state.interop.cx.lock().Flush();

                texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                    None,
                    DXGI_SHARED_RESOURCE_READ.0,
                    None,
                )?
            };

            state.commit_overlay_texture(Some(SharedTextureHandle::Nt(handle.0 as _)))?;
            Ok::<_, anyhow::Error>(())
        }) else {
            continue;
        };

        res?;
    }

    Ok(())
}

fn hide_overlay() -> anyhow::Result<()> {
    for id in Surfaces::iter() {
        let Some(res) = Surfaces::state(id, |state| state.commit_overlay_texture(None)) else {
            continue;
        };

        res?;
    }
    Ok(())
}

struct MyApp {
    render_overlay: bool,
    name: String,
    age: u32,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            if ui.button("Toggle overlay").clicked() {
                self.render_overlay ^= true;
                if self.render_overlay {
                    show_overlay().expect("Show overlay texture");
                } else {
                    hide_overlay().expect("Hide overlay texture");
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
}
