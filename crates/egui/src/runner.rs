use core::error::Error;
use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use asdf_overlay::event_sink::OverlayEventSink;
use asdf_overlay_event::SurfaceEvent;
use asdf_overlay_window::{Backends, window::ListenInputFlags};
use asdf_overlay_window_event::{
    WindowEvent,
    input::{
        CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, InputEvent, Key,
        KeyInputState, KeyboardInput, ScrollAxis,
    },
};
use egui::{Context, ImeEvent, Modifiers, MouseWheelUnit, PointerButton, RawInput, TouchPhase};
use egui_directx11::split_output;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{App, CreationContext, OverlayContext, event::Event, state::SurfaceState};

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
        let egui_cx = Context::default();
        let cx = CreationContext { egui_cx };
        let app = setup_fn(&cx).await?;

        Ok(inner(cx.egui_cx, window, (tx, rx), app).await?)
    })
}

async fn inner(
    egui_cx: Context,
    window: Arc<Backends>,
    (tx, mut rx): (UnboundedSender<Event>, UnboundedReceiver<Event>),
    mut app: impl App,
) -> anyhow::Result<()> {
    egui_cx.set_request_repaint_callback({
        let tx = tx.clone();
        move |info| {
            _ = tx.send(Event::from(info));
        }
    });

    let mut surface = next_main_surface(&mut rx, &egui_cx)
        .await
        .context("waiting for main surface")?;
    init_windows(&window);

    let cx = OverlayContext { windows: window };
    let mut input = RawInput {
        viewport_id: egui_cx.viewport_id(),
        screen_rect: Some(egui::Rect {
            min: (0.0, 0.0).into(),
            max: (surface.width as f32, surface.height as f32).into(),
        }),
        focused: true,
        modifiers: egui_cx.input(|state| state.modifiers),
        ..RawInput::default()
    };

    let start = Instant::now();
    while let Some(event) = rx.recv().await {
        match event {
            Event::Overlay(event) => {
                overlay_event(&egui_cx, &mut rx, &mut surface, &mut input, event)
                    .await
                    .context("handling overlay event")?
            }

            Event::Window(event) => window_event(&egui_cx, &cx, &mut input, event)
                .await
                .context("handling window event")?,

            Event::RequestRepaint(info) => {
                let cumulative_pass_nr = egui_cx.cumulative_pass_nr();
                if info.current_cumulative_pass_nr != cumulative_pass_nr
                    && info.current_cumulative_pass_nr + 1 != cumulative_pass_nr
                {
                    continue;
                }
                input.time = Some(start.elapsed().as_secs_f64());

                app.logic(&egui_cx, &cx);
                let output = egui_cx.run_ui(input.take(), |ui| {
                    app.ui(ui, &cx);
                });

                let clear_color = app.clear_color(&egui_cx.global_style().visuals);
                let (renderer_output, _, _) = split_output(output);
                surface
                    .render(renderer_output, &egui_cx, clear_color)
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

async fn window_event(
    egui_cx: &Context,
    cx: &OverlayContext,
    raw_input: &mut RawInput,
    event: asdf_overlay_window_event::Event,
) -> anyhow::Result<()> {
    let (id, event) = match event {
        asdf_overlay_window_event::Event::Window { id, event } => (id, event),

        asdf_overlay_window_event::Event::InputBlockingEnded => {
            return Ok(());
        }
    };

    match event {
        WindowEvent::Added { .. } => {
            cx.windows.window(id, |state| {
                state.set_input_flags(ListenInputFlags::all());
            });
        }

        WindowEvent::Input(event) => {
            match event {
                InputEvent::Cursor(input) => handle_cursor_input(raw_input, input),
                InputEvent::Keyboard(input) => handle_keyboard_input(raw_input, input),
            }
            egui_cx.request_repaint();
        }

        _ => {}
    }

    Ok(())
}

fn handle_cursor_input(raw_input: &mut RawInput, input: CursorInput) {
    let inputs = &mut raw_input.events;

    match input.event {
        CursorEvent::Move => {
            inputs.push(egui::Event::PointerMoved(
                (input.pos.x as f32, input.pos.y as f32).into(),
            ));
        }

        CursorEvent::Leave => inputs.push(egui::Event::PointerGone),

        CursorEvent::Action { state, action } => {
            inputs.push(egui::Event::PointerButton {
                pos: (input.pos.x as f32, input.pos.y as f32).into(),
                button: match action {
                    CursorAction::Left => PointerButton::Primary,
                    CursorAction::Right => PointerButton::Secondary,
                    CursorAction::Middle => PointerButton::Middle,
                    CursorAction::Back => PointerButton::Extra1,
                    CursorAction::Forward => PointerButton::Extra2,
                },
                pressed: matches!(state, CursorInputState::Pressed { .. }),
                modifiers: raw_input.modifiers,
            });
        }

        CursorEvent::Scroll { axis, delta } => inputs.push(egui::Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: match axis {
                ScrollAxis::X => (delta as f32, 0.0).into(),
                ScrollAxis::Y => (0.0, delta as f32).into(),
            },
            phase: TouchPhase::Move,
            modifiers: raw_input.modifiers,
        }),

        _ => {}
    }
}

fn handle_keyboard_input(raw: &mut RawInput, input: KeyboardInput) {
    let inputs = &mut raw.events;

    match input {
        KeyboardInput::Key { key, state } => {
            let Some(key) = conv_key(key) else {
                return;
            };

            let pressed = state == KeyInputState::Pressed;
            update_modifiers(&mut raw.modifiers, key, pressed);

            inputs.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: raw.modifiers,
            });
        }

        KeyboardInput::Char(ch) => {
            if ch.is_ascii_control() {
                return;
            }

            inputs.push(egui::Event::Text(ch.to_string()))
        }

        KeyboardInput::Ime(ime) => match ime {
            Ime::Compose { text, caret } => {
                let range = caret..text.chars().count();
                inputs.push(egui::Event::Ime(ImeEvent::Preedit {
                    text,
                    active_range_chars: Some(range),
                }));
            }

            Ime::Commit(text) => {
                inputs.push(egui::Event::Ime(ImeEvent::Commit(text)));
            }

            _ => {}
        },
    }
}

fn update_modifiers(modifiers: &mut Modifiers, key: egui::Key, pressed: bool) {
    match key {
        egui::Key::ShiftLeft | egui::Key::ShiftRight => modifiers.shift = pressed,
        egui::Key::ControlLeft | egui::Key::ControlRight => modifiers.ctrl = pressed,
        egui::Key::AltLeft | egui::Key::AltRight => modifiers.alt = pressed,
        egui::Key::SuperLeft | egui::Key::SuperRight => modifiers.command = pressed,

        _ => {}
    }
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

        return SurfaceState::new(cx, id, info.gpu_id, width, height);
    }

    anyhow::bail!("surface not found");
}

fn conv_key(key: Key) -> Option<egui::Key> {
    Some(match key.code.get() {
        8 => egui::Key::Backspace,
        9 => egui::Key::Tab,
        13 => egui::Key::Enter,
        16 => {
            if key.extended {
                egui::Key::ShiftRight
            } else {
                egui::Key::ShiftLeft
            }
        }
        17 => {
            if key.extended {
                egui::Key::ControlRight
            } else {
                egui::Key::ControlLeft
            }
        }
        18 => {
            if key.extended {
                egui::Key::AltRight
            } else {
                egui::Key::AltLeft
            }
        }
        27 => egui::Key::Escape,
        32 => egui::Key::Space,
        33 => egui::Key::PageUp,
        34 => egui::Key::PageDown,
        35 => egui::Key::End,
        36 => egui::Key::Home,
        37 => egui::Key::ArrowLeft,
        38 => egui::Key::ArrowUp,
        39 => egui::Key::ArrowRight,
        45 => egui::Key::Insert,
        46 => egui::Key::Delete,
        48 => egui::Key::Num0,
        49 => egui::Key::Num1,
        50 => egui::Key::Num2,
        51 => egui::Key::Num3,
        52 => egui::Key::Num4,
        53 => egui::Key::Num5,
        54 => egui::Key::Num6,
        55 => egui::Key::Num7,
        56 => egui::Key::Num8,
        57 => egui::Key::Num9,
        65 => egui::Key::A,
        66 => egui::Key::B,
        67 => egui::Key::C,
        68 => egui::Key::D,
        69 => egui::Key::E,
        70 => egui::Key::F,
        71 => egui::Key::G,
        72 => egui::Key::H,
        73 => egui::Key::I,
        74 => egui::Key::J,
        75 => egui::Key::K,
        76 => egui::Key::L,
        77 => egui::Key::M,
        78 => egui::Key::N,
        79 => egui::Key::O,
        80 => egui::Key::P,
        81 => egui::Key::Q,
        82 => egui::Key::R,
        83 => egui::Key::S,
        84 => egui::Key::T,
        85 => egui::Key::U,
        86 => egui::Key::V,
        87 => egui::Key::W,
        88 => egui::Key::X,
        89 => egui::Key::Y,
        90 => egui::Key::Z,
        91 => egui::Key::SuperLeft,
        92 => egui::Key::SuperRight,
        96 => egui::Key::Num0,
        97 => egui::Key::Num1,
        98 => egui::Key::Num2,
        99 => egui::Key::Num3,
        100 => egui::Key::Num4,
        101 => egui::Key::Num5,
        102 => egui::Key::Num6,
        103 => egui::Key::Num7,
        104 => egui::Key::Num8,
        105 => egui::Key::Num9,
        106 => egui::Key::Delete,
        108 => egui::Key::Minus,
        109 => egui::Key::Minus,
        110 => egui::Key::Period,
        111 => egui::Key::Slash,
        112 => egui::Key::F1,
        113 => egui::Key::F2,
        114 => egui::Key::F3,
        115 => egui::Key::F4,
        116 => egui::Key::F5,
        117 => egui::Key::F6,
        118 => egui::Key::F7,
        119 => egui::Key::F8,
        120 => egui::Key::F9,
        121 => egui::Key::F10,
        122 => egui::Key::F11,
        123 => egui::Key::F12,
        124 => egui::Key::F13,
        125 => egui::Key::F14,
        126 => egui::Key::F15,
        127 => egui::Key::F16,
        128 => egui::Key::F17,
        129 => egui::Key::F18,
        130 => egui::Key::F19,
        131 => egui::Key::F20,
        132 => egui::Key::F21,
        133 => egui::Key::F22,
        134 => egui::Key::F23,
        135 => egui::Key::F24,
        186 => egui::Key::Semicolon,
        187 => egui::Key::Equals,
        188 => egui::Key::Comma,
        189 => egui::Key::Minus,
        190 => egui::Key::Period,
        191 => egui::Key::Slash,
        192 => egui::Key::Backtick,
        219 => egui::Key::OpenCurlyBracket,
        220 => egui::Key::Backslash,
        221 => egui::Key::CloseCurlyBracket,
        222 => egui::Key::Quote,
        225 => egui::Key::AltRight,

        // Unknown key code
        _ => return None,
    })
}
