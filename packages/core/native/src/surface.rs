use core::cell::RefCell;

use asdf_overlay_client::{event::GpuLuid, surface::OverlaySurface};
use bytemuck::pod_read_unaligned;
use neon::{
    prelude::{Context, Cx, FunctionContext, ModuleContext},
    result::{JsResult, NeonResult},
    types::{
        Finalize, JsBox, JsBuffer, JsNumber, JsObject, JsUndefined, JsValue, Value,
        buffer::TypedArray,
    },
};

use crate::{
    conv::{deserialize_copy_rect, deserialize_gpu_luid, serialize_handle_update},
    util::create_adapter_by_luid,
};

struct Surface(RefCell<Option<OverlaySurface>>);

impl Surface {
    pub fn new(luid: Option<GpuLuid>) -> anyhow::Result<Self> {
        let adapter = luid.map(create_adapter_by_luid).transpose()?.flatten();
        let surface = OverlaySurface::new(adapter.as_ref())?;
        Ok(Self(RefCell::new(Some(surface))))
    }

    pub fn with_mut<R>(
        &self,
        cx: &mut Cx,
        f: impl FnOnce(&mut OverlaySurface) -> R,
    ) -> NeonResult<R> {
        match *self.0.borrow_mut() {
            Some(ref mut v) => Ok(f(v)),
            None => cx.throw_error("Surface is destroyed"),
        }
    }

    pub fn destroy(&self, cx: &mut Cx) -> NeonResult<()> {
        match self.0.borrow_mut().take() {
            Some(_) => Ok(()),
            None => cx.throw_error("Surface is already destroyed"),
        }
    }
}

impl Finalize for Surface {
    fn finalize<'a, C: Context<'a>>(self, _: &mut C) {}
}

fn surface_create(mut cx: FunctionContext) -> JsResult<JsBox<Surface>> {
    let luid = match cx.argument_opt(0) {
        Some(v) => {
            let obj = v.downcast_or_throw::<JsObject, _>(&mut cx)?;
            Some(deserialize_gpu_luid(&mut cx, &obj)?)
        }
        None => None,
    };

    let surface = Surface::new(luid)
        .or_else(|err| cx.throw_error(format!("Failed to create surface. {err:?}")))?;
    Ok(cx.boxed(surface))
}

fn surface_update_shtex(mut cx: FunctionContext) -> JsResult<JsValue> {
    let surface = cx.argument::<JsBox<Surface>>(0)?;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let height = cx.argument::<JsNumber>(2)?.value(&mut cx) as u32;
    let handle = pod_read_unaligned::<usize>(cx.argument::<JsBuffer>(3)?.as_slice(&cx));
    let rect = cx
        .argument_opt(4)
        .filter(|v| !v.is_a::<JsUndefined, _>(&mut cx))
        .map(|v| {
            let obj = v.downcast_or_throw::<JsObject, _>(&mut cx)?;
            deserialize_copy_rect(&mut cx, &obj)
        })
        .transpose()?;

    let update = surface
        .with_mut(&mut cx, |surface| {
            surface.update_from_nt_shared(width, height, handle as u32, rect)
        })?
        .or_else(|err| cx.throw_error(format!("Failed to update from shared handle. {err:?}")))?;

    match update {
        Some(update) => Ok(serialize_handle_update(&mut cx, update)?.as_value(&mut cx)),
        None => Ok(cx.undefined().as_value(&mut cx)),
    }
}

fn surface_update_bitmap(mut cx: FunctionContext) -> JsResult<JsValue> {
    let surface = cx.argument::<JsBox<Surface>>(0)?;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let data = cx.argument::<JsBuffer>(2)?.as_slice(&cx).to_vec();

    let update = surface
        .with_mut(&mut cx, |surface| surface.update_bitmap(width, &data))?
        .or_else(|err| cx.throw_error(format!("Failed to update from shared handle. {err:?}")))?;

    match update {
        Some(update) => Ok(serialize_handle_update(&mut cx, update)?.as_value(&mut cx)),
        None => Ok(cx.undefined().as_value(&mut cx)),
    }
}

fn surface_clear(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let surface = cx.argument::<JsBox<Surface>>(0)?;
    surface.with_mut(&mut cx, |surface| {
        surface.clear();
    })?;
    Ok(cx.undefined())
}

fn surface_destroy(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    cx.argument::<JsBox<Surface>>(0)?.destroy(&mut cx)?;
    Ok(cx.undefined())
}

pub fn export_module_functions(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("surfaceCreate", surface_create)?;
    cx.export_function("surfaceUpdateBitmap", surface_update_bitmap)?;
    cx.export_function("surfaceUpdateShtex", surface_update_shtex)?;
    cx.export_function("surfaceClear", surface_clear)?;
    cx.export_function("surfaceDestroy", surface_destroy)?;
    Ok(())
}
