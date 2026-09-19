use core::error::Error;
use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use asdf_overlay::event_sink::OverlayEventSink;
use asdf_overlay_event::{SurfaceEvent, SurfaceInfo};
use asdf_overlay_window::{Backends, window::ListenInputFlags};
use egui::{Context, RawInput};
use egui_directx11::split_output;
use flume::{Receiver, Sender};

use crate::{
    App, CreationContext, OverlayContext, event::Event, state::State, window::window_event,
};

/// Initialize the overlay and run the application.
///
/// Renders on the first observed surface, waiting for another if it is destroyed.
/// Call once per process, outside the loader lock. Returning or cancelling leaves
/// graphics hooks installed.
///
/// # Panics
/// Panics if the window backend has already been initialized.
pub async fn run_app<T>(
    setup_fn: impl AsyncFnOnce(&CreationContext) -> Result<T, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>>
where
    T: App + 'static,
{
    let (tx, rx) = flume::unbounded::<Event>();

    let window = Backends::new({
        let tx = tx.clone();

        move |event| {
            _ = tx.send(Event::from(event));
        }
    })
    .context("initializing windowing")?;
    let window = Arc::new(window);

    OverlayEventSink::set({
        let tx = tx.clone();
        move |event| {
            _ = tx.send(Event::from(event));
        }
    });
    asdf_overlay::initialize().context("initializing overlay")?;

    let egui_cx = Context::default();
    let cx = CreationContext { egui_cx };
    let app = setup_fn(&cx).await?;

    Ok(inner(cx.egui_cx, window, (tx, rx), app).await?)
}

async fn inner(
    egui_cx: Context,
    windows: Arc<Backends>,
    (tx, mut rx): (Sender<Event>, Receiver<Event>),
    mut app: impl App,
) -> anyhow::Result<()> {
    egui_cx.set_request_repaint_callback({
        let tx = tx.clone();
        move |info| {
            _ = tx.send(Event::from(info));
        }
    });

    let surface = main_surface(&mut rx)
        .await
        .context("waiting for main surface")?;
    let mut state = State::new(egui_cx, surface.info, surface.width, surface.height)
        .context("creating state")?;
    state.commit_to_surface(surface.id);

    init_windows(&windows);

    let mut cx = OverlayContext {
        windows,
        info: surface.info,
    };
    let mut input = RawInput {
        viewport_id: state.egui_cx.viewport_id(),
        screen_rect: Some(egui::Rect {
            min: (0.0, 0.0).into(),
            max: (surface.width as f32, surface.height as f32).into(),
        }),
        focused: true,
        ..RawInput::default()
    };
    let mut surface = Some(surface);

    let start = Instant::now();
    while let Ok(event) = rx.recv_async().await {
        match event {
            Event::Overlay(event) => {
                overlay_event(&mut cx, &mut surface, &mut state, &mut input, event)
                    .await
                    .context("handling overlay event")?
            }

            Event::Window(event) => match event {
                asdf_overlay_window_event::Event::Window { id, event } => {
                    window_event(&state.egui_cx, &cx, &mut input, id, event)
                        .await
                        .context("handling window event")?
                }

                asdf_overlay_window_event::Event::InputBlockingEnded => {
                    app.on_input_blocking_ended();
                    state.egui_cx.request_repaint();
                }
            },

            Event::RequestRepaint(info) => {
                let cumulative_pass_nr = state.egui_cx.cumulative_pass_nr();
                if info.current_cumulative_pass_nr != cumulative_pass_nr
                    && info.current_cumulative_pass_nr + 1 != cumulative_pass_nr
                {
                    continue;
                }
                input.time = Some(start.elapsed().as_secs_f64());

                app.logic(&state.egui_cx, &cx);
                let output = state.egui_cx.run_ui(input.take(), |ui| {
                    app.ui(ui, &cx);
                });

                let clear_color = app.clear_color(&state.egui_cx.global_style().visuals);
                let (renderer_output, _, _) = split_output(output);
                state
                    .render(renderer_output, clear_color)
                    .context("rendering failed")?;
            }
        }
    }
    Ok(())
}

fn init_windows(window: &Backends) {
    for id in window.windows() {
        window.window(id, |state| {
            state.set_input_flags(ListenInputFlags::all());
        });
    }
}

async fn overlay_event(
    cx: &mut OverlayContext,
    surface: &mut Option<Surface>,
    state: &mut State,
    input: &mut RawInput,
    event: asdf_overlay_event::Event,
) -> anyhow::Result<()> {
    let asdf_overlay_event::Event::Surface { id, event } = event;

    match event {
        SurfaceEvent::Added {
            width,
            height,
            info,
        } => {
            if surface.is_some() {
                return Ok(());
            }

            input.screen_rect = Some(egui::Rect {
                min: (0.0, 0.0).into(),
                max: (width as f32, height as f32).into(),
            });
            state.resize(width, height);
            state.commit_to_surface(id);

            cx.info = info;
            *surface = Some(Surface {
                id,
                width,
                height,
                info,
            });
        }

        SurfaceEvent::Resized { width, height } => {
            let Some(inner) = surface.as_ref() else {
                return Ok(());
            };

            if id != inner.id {
                return Ok(());
            }

            state.resize(width, height);
            input.screen_rect = Some(egui::Rect {
                min: (0.0, 0.0).into(),
                max: (width as f32, height as f32).into(),
            });
        }

        SurfaceEvent::Destroyed => {
            let Some(inner) = surface.as_ref() else {
                return Ok(());
            };

            if id != inner.id {
                return Ok(());
            }

            *surface = None;
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct Surface {
    id: u64,
    width: u32,
    height: u32,
    info: SurfaceInfo,
}

async fn main_surface(rx: &mut Receiver<Event>) -> anyhow::Result<Surface> {
    while let Ok(event) = rx.recv_async().await {
        let Event::Overlay(asdf_overlay_event::Event::Surface { id, event }) = event else {
            continue;
        };

        let asdf_overlay_event::SurfaceEvent::Added {
            width,
            height,
            info,
        } = event
        else {
            continue;
        };

        return Ok(Surface {
            id,
            width,
            height,
            info,
        });
    }

    anyhow::bail!("surface not found");
}
