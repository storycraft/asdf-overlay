use core::sync::atomic::{AtomicU32, Ordering};

use anyhow::Context as AnyhowContext;
use asdf_overlay_client::{event::GpuLuid, surface::OverlaySurface};
use bytemuck::pod_read_unaligned;
use neon::{
    prelude::{Context, FunctionContext, ModuleContext},
    result::{JsResult, NeonResult},
    types::{JsBuffer, JsNumber, JsObject, JsUndefined, buffer::TypedArray},
};
use once_cell::sync::Lazy;

use crate::{
    FxDashMap,
    conv::{deserialize_copy_rect, deserialize_gpu_luid},
    util::create_adapter_by_luid,
};

struct SurfaceStore {
    next_id: AtomicU32,
    overlay_map: FxDashMap<u32, OverlaySurface>,
}

impl SurfaceStore {
    fn create(&self, luid: Option<GpuLuid>) -> anyhow::Result<u32> {
        let adapter = luid.map(create_adapter_by_luid).transpose()?.flatten();
        let surface = OverlaySurface::new(adapter.as_ref())?;

        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        self.overlay_map.insert(id, surface);
        Ok(id)
    }

    fn with_mut<R>(&self, id: u32, f: impl FnOnce(&mut OverlaySurface) -> R) -> anyhow::Result<R> {
        let mut surface = self
            .overlay_map
            .get_mut(&id)
            .context("Invalid surface id.")?;
        Ok(f(&mut surface))
    }

    fn destroy(&self, id: u32) -> bool {
        self.overlay_map.remove(&id).is_some()
    }
}

static STORE: Lazy<SurfaceStore> = Lazy::new(|| SurfaceStore {
    next_id: AtomicU32::new(0),
    overlay_map: FxDashMap::default(),
});

fn surface_create(mut cx: FunctionContext) -> JsResult<JsNumber> {
    let luid = match cx.argument_opt(0) {
        Some(v) => {
            let obj = v.downcast_or_throw::<JsObject, _>(&mut cx)?;
            Some(deserialize_gpu_luid(&mut cx, &obj)?)
        }
        None => None,
    };

    Ok(STORE
        .create(luid)
        .map(|id| cx.number(id))
        .or_else(|err| cx.throw_error(format!("Failed to create surface. {err:?}")))?)
}

fn surface_update_shtex(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
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

    let res = STORE.with_mut(id, |surface| {
        surface.update_from_shared(width, height, handle as u32, rect)
    });
    // TODO:: update
    Ok(cx.undefined())
}

fn surface_update_bitmap(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let data = cx.argument::<JsBuffer>(2)?.as_slice(&cx).to_vec();

    STORE.with_mut(id, |surface| surface.update_bitmap(width, &data));
    // TODO:: update
    Ok(cx.undefined())
}

fn surface_clear(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;

    STORE
        .with_mut(id, |surface| {
            surface.clear();
        })
        .or_else(|err| cx.throw_error(format!("Failed to clear surface. {err:?}")))?;
    Ok(cx.undefined())
}

fn surface_destroy(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    STORE.destroy(id);

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
