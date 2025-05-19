use asdf_overlay_client::{
    common::{
        event::{
            ClientEvent, WindowEvent,
            input::{
                CursorAction, CursorEvent, CursorInput, InputEvent, InputState, KeyboardInput,
                ScrollAxis,
            },
        },
        key::Key,
        size::PercentLength,
    },
    ty::{CopyRect, Rect},
};
use neon::{
    handle::Handle,
    object::Object,
    prelude::Context,
    result::{JsResult, NeonResult},
    types::{JsFunction, JsNumber, JsObject, JsString},
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
            WindowEvent::Added { width, height } => {
                builder
                    .arg(cx.string("added"))
                    .arg(cx.number(hwnd))
                    .arg(cx.number(width))
                    .arg(cx.number(height));
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
            WindowEvent::InputBlockingEnded => {
                builder
                    .arg(cx.string("input_blocking_ended"))
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

    match input {
        KeyboardInput::Key { key, state } => {
            let kind = cx.string("Key");
            obj.set(cx, "kind", kind)?;

            let key = serialize_key(cx, key)?;
            obj.set(cx, "key", key)?;

            let state = serialize_input_state(cx, state);
            obj.set(cx, "state", state)?;
        }
        KeyboardInput::Char(ch) => {
            let kind = cx.string("Char");
            obj.set(cx, "kind", kind)?;

            let mut buf = [0_u8; 4];
            let ch = cx.string(ch.encode_utf8(&mut buf));
            obj.set(cx, "ch", ch)?;
        }
    }

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

pub fn deserialize_percent_length<'a>(
    cx: &mut impl Context<'a>,
    obj: &Handle<'a, JsObject>,
) -> NeonResult<PercentLength> {
    let ty = obj.get::<JsString, _, _>(cx, "ty")?.value(cx);
    let value = obj.get::<JsNumber, _, _>(cx, "value")?.value(cx);

    match ty.as_str() {
        "percent" => Ok(PercentLength::Percent(value as _)),
        "length" => Ok(PercentLength::Length(value as _)),

        _ => cx.throw_error("invalid PercentLength type"),
    }
}

pub fn deserialize_copy_rect<'a>(
    cx: &mut impl Context<'a>,
    obj: &Handle<'a, JsObject>,
) -> NeonResult<CopyRect> {
    let dst_x = obj.get::<JsNumber, _, _>(cx, "dstX")?.value(cx) as u32;
    let dst_y = obj.get::<JsNumber, _, _>(cx, "dstY")?.value(cx) as u32;
    let src = obj.get::<JsObject, _, _>(cx, "src")?;

    Ok(CopyRect {
        dst_x,
        dst_y,
        src: deserialize_rect(cx, &src)?,
    })
}

fn deserialize_rect<'a>(cx: &mut impl Context<'a>, obj: &Handle<'a, JsObject>) -> NeonResult<Rect> {
    let x = obj.get::<JsNumber, _, _>(cx, "x")?.value(cx) as u32;
    let y = obj.get::<JsNumber, _, _>(cx, "y")?.value(cx) as u32;
    let width = obj.get::<JsNumber, _, _>(cx, "width")?.value(cx) as u32;
    let height = obj.get::<JsNumber, _, _>(cx, "height")?.value(cx) as u32;

    Ok(Rect {
        x,
        y,
        width,
        height,
    })
}
