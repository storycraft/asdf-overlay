use asdf_overlay_client::common::event::{ClientEvent, WindowEvent};
use neon::{object::Object, prelude::Context, result::JsResult, types::JsObject};

pub fn serialize_event<'a>(
    cx: &mut impl Context<'a>,
    event: ClientEvent,
) -> JsResult<'a, JsObject> {
    let obj = cx.empty_object();

    match event {
        ClientEvent::Window { hwnd, event } => {
            let kind = cx.string("window");
            obj.set(cx, "kind", kind)?;

            let hwnd = cx.number(hwnd);
            obj.set(cx, "hwnd", hwnd)?;

            let inner = cx.empty_object();
            match event {
                WindowEvent::Added => {
                    let kind = cx.string("added");
                    inner.set(cx, "kind", kind)?;
                }

                WindowEvent::Resized { width, height } => {
                    let kind = cx.string("resized");
                    inner.set(cx, "kind", kind)?;

                    let width = cx.number(width);
                    inner.set(cx, "width", width)?;

                    let height = cx.number(height);
                    inner.set(cx, "height", height)?;
                }

                WindowEvent::InputCaptureStart => {}

                WindowEvent::InputCaptureEnd => {}

                WindowEvent::Input(_) => {}

                WindowEvent::Destroyed => {
                    let kind = cx.string("destroyed");
                    inner.set(cx, "kind", kind)?;
                }
            }

            obj.set(cx, "event", inner)?;
        }
    }

    Ok(obj)
}
