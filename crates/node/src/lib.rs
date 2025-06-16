mod conv;
mod util;

use core::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::{path::PathBuf, sync::LazyLock};

use anyhow::Context as AnyhowContext;
use asdf_overlay_client::{
    OverlayDll,
    common::{
        cursor::Cursor,
        event::ClientEvent,
        ipc::client::{IpcClientConn, IpcClientEventStream},
        request::{
            BlockInput, ListenInput, SetAnchor, SetBlockingCursor, SetMargin, SetPosition,
            UpdateSharedHandle,
        },
    },
    inject,
    surface::OverlaySurface,
};
use bytemuck::pod_read_unaligned;
use conv::{deserialize_copy_rect, deserialize_percent_length, emit_event};
use dashmap::DashMap;
use mimalloc::MiMalloc;
use neon::{prelude::*, types::buffer::TypedArray};
use num::FromPrimitive;
use once_cell::sync::OnceCell;
use rustc_hash::FxBuildHasher;
use tokio::runtime::Runtime;
use util::{try_with_ipc, with_rt};

struct Overlay {
    surface: tokio::sync::Mutex<OverlaySurface>,
    ipc: tokio::sync::Mutex<IpcClientConn>,
    event: tokio::sync::Mutex<IpcClientEventStream>,
}

struct Manager {
    next_id: AtomicU32,
    map: DashMap<u32, Overlay, FxBuildHasher>,
}

impl Manager {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(0),
            map: DashMap::with_hasher(FxBuildHasher),
        }
    }

    async fn attach(
        &self,
        dll_dir: PathBuf,
        pid: u32,
        timeout: Option<Duration>,
    ) -> anyhow::Result<u32> {
        let surface = OverlaySurface::new().context("cannot create dx11 device")?;
        let (ipc, stream) = inject(
            pid,
            OverlayDll {
                x64: Some(&dll_dir.join("asdf_overlay-x64.dll")),
                x86: Some(&dll_dir.join("asdf_overlay-x86.dll")),
                arm64: Some(&dll_dir.join("asdf_overlay-aarch64.dll")),
            },
            timeout,
        )
        .await
        .context("cannot inject to the process")?;

        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        self.map.insert(
            id,
            Overlay {
                surface: tokio::sync::Mutex::new(surface),
                ipc: tokio::sync::Mutex::new(ipc),
                event: tokio::sync::Mutex::new(stream),
            },
        );

        Ok(id)
    }

    async fn with<R>(&self, id: u32, f: impl AsyncFnOnce(&Overlay) -> R) -> anyhow::Result<R> {
        let overlay = self.map.get(&id).context("invalid id")?;
        Ok(f(&*overlay).await)
    }

    fn destroy(&self, id: u32) -> anyhow::Result<()> {
        self.map.remove(&id).context("invalid id")?;

        Ok(())
    }
}

static MANAGER: LazyLock<Manager> = LazyLock::new(Manager::new);

fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(format!("{err:?}"))))
}

fn attach(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let dll_dir = cx.argument::<JsString>(0)?.value(&mut cx);
    let pid = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let timeout = cx
        .argument_opt(2)
        .filter(|v| !v.is_a::<JsUndefined, _>(&mut cx))
        .map(|v| v.downcast_or_throw::<JsNumber, _>(&mut cx))
        .transpose()?
        .map(|timeout| Duration::from_millis(timeout.value(&mut cx) as _));

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = MANAGER.attach(PathBuf::from(dll_dir), pid, timeout).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(id) => Ok(JsNumber::new(&mut cx, id)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_set_position(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let x = cx.argument::<JsObject>(1)?;
    let x = deserialize_percent_length(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(2)?;
    let y = deserialize_percent_length(&mut cx, &y)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.set_position(SetPosition { x, y }).await?;
            Ok(())
        }),
    )
}

fn overlay_set_anchor(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let x = cx.argument::<JsObject>(1)?;
    let x = deserialize_percent_length(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(2)?;
    let y = deserialize_percent_length(&mut cx, &y)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.set_anchor(SetAnchor { x, y }).await?;
            Ok(())
        }),
    )
}

fn overlay_set_margin(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let top = cx.argument::<JsObject>(1)?;
    let top = deserialize_percent_length(&mut cx, &top)?;
    let right = cx.argument::<JsObject>(2)?;
    let right = deserialize_percent_length(&mut cx, &right)?;
    let bottom = cx.argument::<JsObject>(3)?;
    let bottom = deserialize_percent_length(&mut cx, &bottom)?;
    let left = cx.argument::<JsObject>(4)?;
    let left = deserialize_percent_length(&mut cx, &left)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.set_margin(SetMargin {
                top,
                right,
                bottom,
                left,
            })
            .await?;
            Ok(())
        }),
    )
}

fn overlay_destroy(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;

    match MANAGER.destroy(id) {
        Ok(_) => Ok(JsUndefined::new(&mut cx)),
        Err(err) => cx.throw_error(format!("{err:?}")),
    }
}

fn overlay_update_bitmap(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let data = cx.argument::<JsBuffer>(2)?.as_slice(&cx).to_vec();

    with_rt(&mut cx, async move {
        MANAGER
            .with(id, async move |overlay| {
                if let Some(shared) = overlay.surface.lock().await.update_bitmap(width, &data)? {
                    overlay.ipc.lock().await.update_shtex(shared).await?;
                }

                Ok::<_, anyhow::Error>(())
            })
            .await??;
        Ok(())
    })
}

fn overlay_update_shtex(mut cx: FunctionContext) -> JsResult<JsPromise> {
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

    with_rt(&mut cx, async move {
        MANAGER
            .with(id, async move |overlay| {
                if let Some(shared) = overlay.surface.lock().await.update_from_nt_shared(
                    width,
                    height,
                    handle as _,
                    rect,
                )? {
                    overlay.ipc.lock().await.update_shtex(shared).await?;
                }

                Ok::<_, anyhow::Error>(())
            })
            .await??;

        Ok(())
    })
}

fn overlay_clear_surface(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;

    with_rt(&mut cx, async move {
        MANAGER
            .with(id, async move |overlay| {
                overlay.surface.lock().await.clear();
                overlay
                    .ipc
                    .lock()
                    .await
                    .update_shtex(UpdateSharedHandle { handle: None })
                    .await?;

                Ok::<_, anyhow::Error>(())
            })
            .await??;

        Ok(())
    })
}

fn overlay_call_next_event(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let emitter = cx.argument::<JsObject>(1)?.root(&mut cx);
    let emit = cx.argument::<JsFunction>(2)?.root(&mut cx);

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = async move {
            let event: Option<ClientEvent> = MANAGER
                .with(id, async move |overlay| {
                    overlay.event.lock().await.recv().await
                })
                .await?;
            Ok::<_, anyhow::Error>(event)
        }
        .await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(Some(event)) => {
                let emitter = emitter.into_inner(&mut cx);
                let emit = emit.into_inner(&mut cx);
                emit_event(&mut cx, event, emitter, emit)?;

                Ok(cx.boolean(true))
            }
            Ok(None) => Ok(cx.boolean(false)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_listen_input(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let hwnd = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let cursor = cx.argument::<JsBoolean>(2)?.value(&mut cx);
    let keyboard = cx.argument::<JsBoolean>(3)?.value(&mut cx);

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.listen_input(ListenInput {
                hwnd,
                cursor,
                keyboard,
            })
            .await?;

            Ok(())
        }),
    )
}

fn overlay_block_input(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let hwnd = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let block = cx.argument::<JsBoolean>(2)?.value(&mut cx);

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.block_input(BlockInput { hwnd, block }).await?;

            Ok(())
        }),
    )
}

fn overlay_set_blocking_cursor(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let hwnd = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let cursor = cx
        .argument_opt(2)
        .filter(|v| !v.is_a::<JsUndefined, _>(&mut cx))
        .map(|v| Ok(v.downcast_or_throw::<JsNumber, _>(&mut cx)?.value(&mut cx) as u32))
        .transpose()?;

    let cursor = match cursor {
        Some(discriminant) => {
            let Some(cursor) = Cursor::from_u32(discriminant) else {
                return cx.throw_error("invalid cursor value");
            };
            Some(cursor)
        }

        None => None,
    };

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.set_blocking_cursor(SetBlockingCursor { hwnd, cursor })
                .await?;

            Ok(())
        }),
    )
}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("attach", attach)?;

    cx.export_function("overlaySetPosition", overlay_set_position)?;
    cx.export_function("overlaySetAnchor", overlay_set_anchor)?;
    cx.export_function("overlaySetMargin", overlay_set_margin)?;
    cx.export_function("overlayListenInput", overlay_listen_input)?;
    cx.export_function("overlayBlockInput", overlay_block_input)?;
    cx.export_function("overlaySetBlockingCursor", overlay_set_blocking_cursor)?;

    cx.export_function("overlayUpdateBitmap", overlay_update_bitmap)?;
    cx.export_function("overlayUpdateShtex", overlay_update_shtex)?;
    cx.export_function("overlayClearSurface", overlay_clear_surface)?;

    cx.export_function("overlayCallNextEvent", overlay_call_next_event)?;

    cx.export_function("overlayDestroy", overlay_destroy)?;
    Ok(())
}
