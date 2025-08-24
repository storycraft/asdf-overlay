use asdf_overlay_client::{
    common::size::PercentLength,
    event::{
        ServerEvent, GpuLuid, WindowEvent,
        input::{
            CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, ImeCandidateList,
            InputEvent, Key, KeyInputState, KeyboardInput, ScrollAxis,
        },
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
    event: ServerEvent,
    emitter: Handle<'a, JsObject>,
    emit: Handle<'a, JsFunction>,
) -> NeonResult<()> {
    let mut call_options = emit.call_with(cx);
    let builder = call_options.this(emitter);

    match event {
        ServerEvent::Window { id, event } => match event {
            WindowEvent::Added {
                width,
                height,
                gpu_id,
            } => {
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

fn serialize_keyboard_input<'a>(cx: &mut Cx<'a>, input: KeyboardInput) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    match input {
        KeyboardInput::Key { key, state } => {
            let kind = cx.string("Key");
            obj.prop(cx, "kind").set(kind)?;

            let key = serialize_key(cx, key)?;
            obj.prop(cx, "key").set(key)?;

            let state = serialize_key_input_state(cx, state);
            obj.prop(cx, "state").set(state)?;
        }
        KeyboardInput::Char(ch) => {
            let kind = cx.string("Char");
            obj.prop(cx, "kind").set(kind)?;

            let mut buf = [0_u8; 4];
            let ch = cx.string(ch.encode_utf8(&mut buf));
            obj.prop(cx, "ch").set(ch)?;
        }
        KeyboardInput::Ime(ime) => {
            let kind = cx.string("Ime");
            obj.prop(cx, "kind").set(kind)?;

            let ime = serialize_ime(cx, ime)?;
            obj.prop(cx, "ime").set(ime)?;
        }
    }

    Ok(obj)
}

fn serialize_cursor_input<'a>(cx: &mut Cx<'a>, input: CursorInput) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let client_x = cx.number(input.client.x);
    obj.prop(cx, "clientX").set(client_x)?;

    let client_y = cx.number(input.client.y);
    obj.prop(cx, "clientY").set(client_y)?;

    let window_x = cx.number(input.client.x);
    obj.prop(cx, "windowX").set(window_x)?;

    let window_y = cx.number(input.client.y);
    obj.prop(cx, "windowY").set(window_y)?;

    match input.event {
        CursorEvent::Enter => {
            let kind = cx.string("Enter");
            obj.prop(cx, "kind").set(kind)?;
        }
        CursorEvent::Leave => {
            let kind = cx.string("Leave");
            obj.prop(cx, "kind").set(kind)?;
        }
        CursorEvent::Action { state, action } => {
            let kind = cx.string("Action");
            obj.prop(cx, "kind").set(kind)?;

            let (state, double_click) = serialize_cursor_input_state(cx, state);
            obj.prop(cx, "state").set(state)?;
            obj.prop(cx, "doubleClick").set(double_click)?;

            let action = match action {
                CursorAction::Left => cx.string("Left"),
                CursorAction::Right => cx.string("Right"),
                CursorAction::Middle => cx.string("Middle"),
                CursorAction::Back => cx.string("Back"),
                CursorAction::Forward => cx.string("Forward"),
            };
            obj.prop(cx, "action").set(action)?;
        }
        CursorEvent::Move => {
            let kind = cx.string("Move");
            obj.prop(cx, "kind").set(kind)?;
        }
        CursorEvent::Scroll { axis, delta } => {
            let kind = cx.string("Scroll");
            obj.prop(cx, "kind").set(kind)?;

            let axis = match axis {
                ScrollAxis::X => cx.string("X"),
                ScrollAxis::Y => cx.string("Y"),
            };
            obj.prop(cx, "axis").set(axis)?;

            let delta = cx.number(delta);
            obj.prop(cx, "delta").set(delta)?;
        }
    }

    Ok(obj)
}

fn serialize_gpu_luid<'a>(cx: &mut Cx<'a>, id: GpuLuid) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let low = cx.number(id.low);
    obj.prop(cx, "low").set(low)?;

    let high = cx.number(id.high);
    obj.prop(cx, "high").set(high)?;

    Ok(obj)
}

fn serialize_key_input_state<'a>(cx: &mut Cx<'a>, state: KeyInputState) -> Handle<'a, JsString> {
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
    obj.prop(cx, "code").set(code)?;

    let extended = cx.boolean(key.extended);
    obj.prop(cx, "extended").set(extended)?;

    Ok(obj)
}

fn serialize_ime<'a>(cx: &mut Cx<'a>, ime: Ime) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let kind = match ime {
        Ime::Enabled { lang, conversion } => {
            let lang = cx.string(lang);
            obj.prop(cx, "lang").set(lang)?;

            let conversion = cx.number(conversion.bits());
            obj.prop(cx, "conversion").set(conversion)?;

            cx.string("Enabled")
        }
        Ime::Changed(lang) => {
            let lang = cx.string(lang);
            obj.prop(cx, "lang").set(lang)?;

            cx.string("Changed")
        }
        Ime::ConversionChanged(conversion) => {
            let conversion = cx.number(conversion.bits());
            obj.prop(cx, "conversion").set(conversion)?;

            cx.string("ConversionChanged")
        }
        Ime::CandidateChanged(candidate_list) => {
            let candidate_list = serialize_candidate_list(cx, candidate_list)?;
            obj.prop(cx, "list").set(candidate_list)?;

            cx.string("CandidateChanged")
        }
        Ime::CandidateClosed => cx.string("CandidateClosed"),
        Ime::Compose { text, caret } => {
            let text = cx.string(text);
            obj.prop(cx, "text").set(text)?;

            let caret = cx.number(caret as f64);
            obj.prop(cx, "caret").set(caret)?;

            cx.string("Compose")
        }
        Ime::Commit(text) => {
            let text = cx.string(text);
            obj.prop(cx, "text").set(text)?;

            cx.string("Commit")
        }
        Ime::Disabled => cx.string("Disabled"),
    };
    obj.prop(cx, "kind").set(kind)?;

    Ok(obj)
}

fn serialize_candidate_list<'a>(
    cx: &mut Cx<'a>,
    candidate_list: ImeCandidateList,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    let page_start_index = cx.number(candidate_list.page_start_index);
    obj.prop(cx, "pageStartIndex").set(page_start_index)?;

    let page_size = cx.number(candidate_list.page_size);
    obj.prop(cx, "pageSize").set(page_size)?;

    let selected_index = cx.number(candidate_list.selected_index);
    obj.prop(cx, "selectedIndex").set(selected_index)?;

    let candidates = {
        let list = cx.empty_array();
        for (i, candidate) in candidate_list.candidates.iter().enumerate() {
            let candidate = cx.string(candidate);
            list.prop(cx, i as u32).set(candidate)?;
        }
        list
    };
    obj.prop(cx, "candidates").set(candidates)?;

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
