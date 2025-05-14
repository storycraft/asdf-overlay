use core::num::NonZeroU8;

use asdf_overlay_client::common::{
    event::{
        ClientEvent, WindowEvent,
        input::{
            CursorAction, CursorEvent, CursorInput, InputEvent, InputState, KeyboardInput,
            ScrollAxis,
        },
    },
    key::Key,
};
use neon::{
    handle::Handle,
    object::Object,
    prelude::Context,
    result::{JsResult, NeonResult},
    types::{JsBoolean, JsFunction, JsNumber, JsObject, JsString},
};

pub fn emit_event<'a>(
    cx: &mut impl Context<'a>,
    event: ClientEvent,
    emitter: Handle<'a, JsObject>,
    emit: Handle<'a, JsFunction>,
) -> NeonResult<()> {
    let mut call_options = emit.call_with(cx);
    let builder = call_options.this(emitter);

    match event {
        ClientEvent::Window { hwnd, event } => match event {
            WindowEvent::Added => {
                builder.arg(cx.string("added")).arg(cx.number(hwnd));
            }
            WindowEvent::Resized { width, height } => {
                builder
                    .arg(cx.string("resized"))
                    .arg(cx.number(hwnd))
                    .arg(cx.number(width))
                    .arg(cx.number(height));
            }
            WindowEvent::Input(event) => match event {
                InputEvent::Cursor(input) => {
                    builder
                        .arg(cx.string("cursor_input"))
                        .arg(cx.number(hwnd))
                        .arg(serialize_cursor_input(cx, input)?);
                }
                InputEvent::Keyboard(input) => {
                    builder
                        .arg(cx.string("keyboard_input"))
                        .arg(cx.number(hwnd))
                        .arg(serialize_keyboard_input(cx, input)?);
                }
            },
            WindowEvent::InputCaptureStart => {
                builder
                    .arg(cx.string("input_capture_start"))
                    .arg(cx.number(hwnd));
            }
            WindowEvent::InputCaptureEnd => {
                builder
                    .arg(cx.string("input_capture_end"))
                    .arg(cx.number(hwnd));
            }
            WindowEvent::Destroyed => {
                builder.arg(cx.string("destroyed")).arg(cx.number(hwnd));
            }
        },
    }

    builder.exec(cx)
}

fn serialize_keyboard_input<'a>(
    cx: &mut impl Context<'a>,
    input: KeyboardInput,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let key = serialize_key(cx, input.key)?;
    obj.set(cx, "key", key)?;

    let state = serialize_input_state(cx, input.state);
    obj.set(cx, "state", state)?;

    Ok(obj)
}

fn serialize_cursor_input<'a>(
    cx: &mut impl Context<'a>,
    input: CursorInput,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let x = cx.number(input.x);
    obj.set(cx, "x", x)?;

    let y = cx.number(input.y);
    obj.set(cx, "y", y)?;

    match input.event {
        CursorEvent::Enter => {
            let kind = cx.string("Enter");
            obj.set(cx, "kind", kind)?;
        }
        CursorEvent::Leave => {
            let kind = cx.string("Leave");
            obj.set(cx, "kind", kind)?;
        }
        CursorEvent::Action { state, action } => {
            let kind = cx.string("Action");
            obj.set(cx, "kind", kind)?;

            let state = serialize_input_state(cx, state);
            obj.set(cx, "state", state)?;

            let action = match action {
                CursorAction::Left => cx.string("Left"),
                CursorAction::Right => cx.string("Right"),
                CursorAction::Middle => cx.string("Middle"),
                CursorAction::Back => cx.string("Back"),
                CursorAction::Forward => cx.string("Forward"),
            };
            obj.set(cx, "action", action)?;
        }
        CursorEvent::Move => {
            let kind = cx.string("Move");
            obj.set(cx, "kind", kind)?;
        }
        CursorEvent::Scroll { axis, delta } => {
            let kind = cx.string("Scroll");
            obj.set(cx, "kind", kind)?;

            let axis = match axis {
                ScrollAxis::X => cx.string("X"),
                ScrollAxis::Y => cx.string("Y"),
            };
            obj.set(cx, "axis", axis)?;

            let delta = cx.number(delta);
            obj.set(cx, "delta", delta)?;
        }
    }

    Ok(obj)
}

fn serialize_input_state<'a>(cx: &mut impl Context<'a>, state: InputState) -> Handle<'a, JsString> {
    match state {
        InputState::Pressed => cx.string("Pressed"),
        InputState::Released => cx.string("Released"),
    }
}

fn serialize_key<'a>(cx: &mut impl Context<'a>, key: Key) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let code = cx.number(key.code.get());
    obj.set(cx, "code", code)?;

    let extended = cx.boolean(key.extended);
    obj.set(cx, "extended", extended)?;

    Ok(obj)
}

pub fn deserialize_key<'a>(
    cx: &mut impl Context<'a>,
    obj: Handle<'a, JsObject>,
) -> NeonResult<Key> {
    let Some(code) = NonZeroU8::new(obj.get::<JsNumber, _, _>(cx, "code")?.value(cx) as u8) else {
        return cx.throw_range_error("code cannot be zero");
    };
    let extended = obj.get::<JsBoolean, _, _>(cx, "extended")?.value(cx);

    Ok(Key { code, extended })
}
