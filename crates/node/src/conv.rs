use asdf_overlay_client::{
    common::size::PercentLength,
    event::{
        input::{
            CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, InputEvent, Key,
            KeyInputState, KeyboardInput, ScrollAxis,
        }, ClientEvent, GpuLuid, WindowEvent
    },
    ty::{CopyRect, Rect},
};
use neon::{
    handle::Handle,
    object::Object,
    prelude::{Context, Cx},
    result::{JsResult, NeonResult},
    types::{JsBoolean, JsFunction, JsNumber, JsObject, JsString},
};

pub fn emit_event<'a>(
    cx: &mut Cx<'a>,
    event: ClientEvent,
    emitter: Handle<'a, JsObject>,
    emit: Handle<'a, JsFunction>,
) -> NeonResult<()> {
    let mut call_options = emit.call_with(cx);
    let builder = call_options.this(emitter);

    match event {
        ClientEvent::Window { id, event } => match event {
            WindowEvent::Added { width, height, gpu_id } => {
                builder
                    .arg(cx.string("added"))
                    .arg(cx.number(id))
                    .arg(cx.number(width))
                    .arg(cx.number(height))
                    .arg(serialize_gpu_luid(cx, gpu_id)?);
            }
            WindowEvent::Resized { width, height } => {
                builder
                    .arg(cx.string("resized"))
                    .arg(cx.number(id))
                    .arg(cx.number(width))
                    .arg(cx.number(height));
            }
            WindowEvent::Input(event) => match event {
                InputEvent::Cursor(input) => {
                    builder
                        .arg(cx.string("cursor_input"))
                        .arg(cx.number(id))
                        .arg(serialize_cursor_input(cx, input)?);
                }
                InputEvent::Keyboard(input) => {
                    builder
                        .arg(cx.string("keyboard_input"))
                        .arg(cx.number(id))
                        .arg(serialize_keyboard_input(cx, input)?);
                }
            },
            WindowEvent::InputBlockingEnded => {
                builder
                    .arg(cx.string("input_blocking_ended"))
                    .arg(cx.number(id));
            }
            WindowEvent::Destroyed => {
                builder.arg(cx.string("destroyed")).arg(cx.number(id));
            }
        },
    }

    builder.exec(cx)
}

fn serialize_keyboard_input<'a>(
    cx: &mut Cx<'a>,
    input: KeyboardInput,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    match input {
        KeyboardInput::Key { key, state } => {
            let kind = cx.string("Key");
            obj.set(cx, "kind", kind)?;

            let key = serialize_key(cx, key)?;
            obj.set(cx, "key", key)?;

            let state = serialize_key_input_state(cx, state);
            obj.set(cx, "state", state)?;
        }
        KeyboardInput::Char(ch) => {
            let kind = cx.string("Char");
            obj.set(cx, "kind", kind)?;

            let mut buf = [0_u8; 4];
            let ch = cx.string(ch.encode_utf8(&mut buf));
            obj.set(cx, "ch", ch)?;
        }
        KeyboardInput::Ime(ime) => {
            let kind = cx.string("Ime");
            obj.set(cx, "kind", kind)?;

            let ime = serialize_ime(cx, ime)?;
            obj.set(cx, "ime", ime)?;
        }
    }

    Ok(obj)
}

fn serialize_cursor_input<'a>(
    cx: &mut Cx<'a>,
    input: CursorInput,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let client_x = cx.number(input.client.x);
    obj.set(cx, "clientX", client_x)?;

    let client_y = cx.number(input.client.y);
    obj.set(cx, "clientY", client_y)?;

    let window_x = cx.number(input.client.x);
    obj.set(cx, "windowX", window_x)?;

    let window_y = cx.number(input.client.y);
    obj.set(cx, "windowY", window_y)?;

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

            let (state, double_click) = serialize_cursor_input_state(cx, state);
            obj.set(cx, "state", state)?;
            obj.set(cx, "doubleClick", double_click)?;

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

fn serialize_gpu_luid<'a>(
    cx: &mut Cx<'a>,
    id: GpuLuid,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let low = cx.number(id.low);
    obj.prop(cx, "low").set(low)?;

    let high = cx.number(id.high);
    obj.prop(cx, "high").set(high)?;

    Ok(obj)
}

fn serialize_key_input_state<'a>(
    cx: &mut Cx<'a>,
    state: KeyInputState,
) -> Handle<'a, JsString> {
    match state {
        KeyInputState::Pressed => cx.string("Pressed"),
        KeyInputState::Released => cx.string("Released"),
    }
}

fn serialize_cursor_input_state<'a>(
    cx: &mut Cx<'a>,
    state: CursorInputState,
) -> (Handle<'a, JsString>, Handle<'a, JsBoolean>) {
    match state {
        CursorInputState::Pressed { double_click } => {
            (cx.string("Pressed"), cx.boolean(double_click))
        }
        CursorInputState::Released => (cx.string("Released"), cx.boolean(false)),
    }
}

fn serialize_key<'a>(cx: &mut Cx<'a>, key: Key) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let code = cx.number(key.code.get());
    obj.set(cx, "code", code)?;

    let extended = cx.boolean(key.extended);
    obj.set(cx, "extended", extended)?;

    Ok(obj)
}

fn serialize_ime<'a>(cx: &mut Cx<'a>, ime: Ime) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let kind = match ime {
        Ime::Enabled { lang, conversion } => {
            let lang = cx.string(lang);
            obj.set(cx, "lang", lang)?;

            let conversion = cx.number(conversion.bits());
            obj.set(cx, "conversion", conversion)?;

            cx.string("Enabled")
        }
        Ime::Changed(lang) => {
            let lang = cx.string(lang);
            obj.set(cx, "lang", lang)?;

            cx.string("Changed")
        }
        Ime::ConversionChanged(conversion) => {
            let conversion = cx.number(conversion.bits());
            obj.set(cx, "conversion", conversion)?;

            cx.string("ConversionChanged")
        }
        Ime::Compose { text, caret } => {
            let text = cx.string(text);
            obj.set(cx, "text", text)?;

            let caret = cx.number(caret as f64);
            obj.set(cx, "caret", caret)?;

            cx.string("Compose")
        }
        Ime::Commit(text) => {
            let text = cx.string(text);
            obj.set(cx, "text", text)?;

            cx.string("Commit")
        }
        Ime::Disabled => cx.string("Disabled"),
    };
    obj.set(cx, "kind", kind)?;

    Ok(obj)
}

pub fn deserialize_percent_length<'a>(
    cx: &mut Cx<'a>,
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
    cx: &mut Cx<'a>,
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

fn deserialize_rect<'a>(cx: &mut Cx<'a>, obj: &Handle<'a, JsObject>) -> NeonResult<Rect> {
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
