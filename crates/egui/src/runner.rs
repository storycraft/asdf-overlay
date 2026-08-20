use core::error::Error;
use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use asdf_overlay::event_sink::OverlayEventSink;
use asdf_overlay_event::SurfaceEvent;
use asdf_overlay_window::Backends;
use egui::{Context, RawInput};
use egui_directx11::split_output;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{App, CreationContext, event::Event, state::SurfaceState};

pub fn run_app<T>(
    setup_fn: impl AsyncFnOnce(&CreationContext) -> Result<T, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>>
where
    T: App + 'static,
{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let window = Arc::new(Backends::new().context("initializing windowing")?);
    asdf_overlay::initialize().context("initializing overlay")?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    rt.spawn({
        let window = Arc::clone(&window);
        let tx = tx.clone();

        async move {
            while let Some(event) = window.recv_async().await {
                _ = tx.send(Event::from(event));
            }
        }
    });
    OverlayEventSink::set({
        let tx = tx.clone();
        move |event| {
            _ = tx.send(Event::from(event));
        }
    });

    rt.block_on(async move {
        let egui_ctx = Context::default();
        let cx = CreationContext { egui_ctx };
        let app = setup_fn(&cx).await?;

        Ok(inner(cx.egui_ctx, window, (tx, rx), app).await?)
    })
}

async fn inner(
    cx: Context,
    _window: Arc<Backends>,
    (tx, mut rx): (UnboundedSender<Event>, UnboundedReceiver<Event>),
    mut app: impl App,
) -> anyhow::Result<()> {
    cx.set_request_repaint_callback({
        let tx = tx.clone();
        move |info| {
            _ = tx.send(Event::from(info));
        }
    });

    let mut surface = next_main_surface(&mut rx, &cx)
        .await
        .context("waiting for main surface")?;

    let mut input = RawInput::default();
    input.screen_rect = Some(egui::Rect {
        min: (0.0, 0.0).into(),
        max: (surface.width as f32, surface.height as f32).into(),
    });

    let start = Instant::now();
    while let Some(event) = rx.recv().await {
        match event {
            Event::Overlay(event) => overlay_event(&cx, &mut rx, &mut surface, &mut input, event)
                .await
                .context("handling overlay event")?,

            Event::Window(_event) => {}

            Event::RequestRepaint(info) => {
                let cumulative_pass_nr = cx.cumulative_pass_nr();
                if info.current_cumulative_pass_nr != cumulative_pass_nr
                    && info.current_cumulative_pass_nr + 1 != cumulative_pass_nr
                {
                    continue;
                }

                input.time = Some(start.elapsed().as_secs_f64());

                app.logic(&cx);

                let output = cx.run_ui(input.take(), |ui| {
                    app.ui(ui);
                });
                let clear_color = app.clear_color(&cx.global_style().visuals);
                let (renderer_output, _, _) = split_output(output);
                surface
                    .render(renderer_output, &cx, clear_color)
                    .context("rendering failed")?;
            }
        }
    }
    Ok(())
}

async fn overlay_event(
    cx: &Context,
    rx: &mut UnboundedReceiver<Event>,
    surface: &mut SurfaceState,
    input: &mut RawInput,
    event: asdf_overlay_event::Event,
) -> anyhow::Result<()> {
    let asdf_overlay_event::Event::Surface { id, event } = event;
    if id != surface.id {
        return Ok(());
    }

    match event {
        SurfaceEvent::Resized { width, height } => {
            surface.resize(cx, width, height);
            input.screen_rect = Some(egui::Rect {
                min: (0.0, 0.0).into(),
                max: (width as f32, height as f32).into(),
            });
        }

        SurfaceEvent::Destroyed => {
            *surface = next_main_surface(rx, cx)
                .await
                .context("waiting for main surface")?;
        }
        _ => {}
    }

    Ok(())
}

async fn next_main_surface(
    rx: &mut UnboundedReceiver<Event>,
    cx: &Context,
) -> anyhow::Result<SurfaceState> {
    while let Some(event) = rx.recv().await {
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

        return Ok(SurfaceState::new(cx, id, info.gpu_id, width, height)?);
    }

    anyhow::bail!("surface not found");
}
