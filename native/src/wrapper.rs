use asdf_overlay_client::common::size::PercentLength;
use neon::{
    object::Object,
    prelude::{Context, FunctionContext},
    result::NeonResult,
    types::{JsNumber, JsObject, JsString},
};

pub fn percent_length_from_object(
    cx: &mut FunctionContext,
    obj: &JsObject,
) -> NeonResult<PercentLength> {
    let ty = obj.get::<JsString, _, _>(cx, "ty")?.value(cx);
    let value = obj.get::<JsNumber, _, _>(cx, "value")?.value(cx);

    match ty.as_str() {
        "percent" => Ok(PercentLength::Percent(value as _)),
        "length" => Ok(PercentLength::Length(value as _)),

        _ => cx.throw_error("invalid PercentLength type"),
    }
}
