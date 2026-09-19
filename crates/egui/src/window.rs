mod conv;

use asdf_overlay_window::window::ListenInputFlags;
use asdf_overlay_window_event::{
    WindowEvent,
    input::{
        CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, InputEvent, KeyInputState,
        KeyboardInput, ScrollAxis,
    },
};
use egui::{Context, ImeEvent, Modifiers, MouseWheelUnit, PointerButton, RawInput, TouchPhase};

use crate::{OverlayContext, window::conv::conv_key};

pub async fn window_event(
    egui_cx: &Context,
    cx: &OverlayContext,
    raw_input: &mut RawInput,
    id: u32,
    event: WindowEvent,
) -> anyhow::Result<()> {
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
