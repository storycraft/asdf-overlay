use core::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::{path::PathBuf, sync::LazyLock};

use super::conv::{deserialize_percent_length, emit_event};
use super::util::with_rt;
use crate::{FxSccMap, conv::deserialize_handle_update, util::runtime};
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::{
    OverlayDll,
    client::{IpcClientConn, IpcClientEventStream},
    common::{
        cursor::Cursor,
        request::{BlockInput, ListenInput, SetAnchor, SetBlockingCursor, SetMargin, SetPosition},
    },
    event::OverlayEvent,
    inject,
};
use neon::prelude::*;
use num::FromPrimitive;

struct Overlay {
    ipc: IpcClientConn,
    event: IpcClientEventStream,
}

struct OverlayStore {
    next_id: AtomicU32,
    overlay_map: FxSccMap<u32, Overlay>,
}

impl OverlayStore {
    async fn attach(
        &self,
        dll_dir: PathBuf,
        pid: u32,
        timeout: Option<Duration>,
    ) -> anyhow::Result<u32> {
        let (ipc, event) = inject(
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
        self.overlay_map.upsert_sync(id, Overlay { ipc, event });

        Ok(id)
    }

    async fn with_mut<R>(
        &self,
        id: u32,
        f: impl AsyncFnOnce(&mut Overlay) -> R,
    ) -> anyhow::Result<R> {
        let mut overlay = self
            .overlay_map
            .get_async(&id)
            .await
            .context("invalid id")?;
        Ok(f(&mut *overlay).await)
    }

    fn destroy(&self, id: u32) -> anyhow::Result<()> {
        self.overlay_map.remove_sync(&id).context("invalid id")?;

        Ok(())
    }
}

static STORE: LazyLock<OverlayStore> = LazyLock::new(|| OverlayStore {
    next_id: AtomicU32::new(0),
    overlay_map: FxSccMap::default(),
});

pub async fn try_with_ipc<T>(
    id: u32,
    f: impl AsyncFnOnce(&mut IpcClientConn) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    STORE
        .with_mut(id, async move |overlay| f(&mut overlay.ipc).await)
        .await?
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
        let res = STORE.attach(PathBuf::from(dll_dir), pid, timeout).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(id) => Ok(JsNumber::new(&mut cx, id)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_set_position(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let x = cx.argument::<JsObject>(2)?;
    let x = deserialize_percent_length(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(3)?;
    let y = deserialize_percent_length(&mut cx, &y)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id).request(SetPosition { x, y }).await?;
            Ok(())
        }),
    )
}

fn overlay_update_handle(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let update = {
        let obj = cx.argument::<JsObject>(2)?;
        deserialize_handle_update(&mut cx, &obj)?
    };

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id).request(update).await?;
            Ok(())
        }),
    )
}

fn overlay_set_anchor(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let x = cx.argument::<JsObject>(2)?;
    let x = deserialize_percent_length(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(3)?;
    let y = deserialize_percent_length(&mut cx, &y)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id).request(SetAnchor { x, y }).await?;
            Ok(())
        }),
    )
}

fn overlay_set_margin(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let top = cx.argument::<JsObject>(2)?;
    let top = deserialize_percent_length(&mut cx, &top)?;
    let right = cx.argument::<JsObject>(3)?;
    let right = deserialize_percent_length(&mut cx, &right)?;
    let bottom = cx.argument::<JsObject>(4)?;
    let bottom = deserialize_percent_length(&mut cx, &bottom)?;
    let left = cx.argument::<JsObject>(5)?;
    let left = deserialize_percent_length(&mut cx, &left)?;

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id)
                .request(SetMargin {
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

    match STORE.destroy(id) {
        Ok(_) => Ok(JsUndefined::new(&mut cx)),
        Err(err) => cx.throw_error(format!("{err:?}")),
    }
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
            let event: Option<OverlayEvent> = STORE
                .with_mut(id, async move |overlay| overlay.event.recv().await)
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
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let cursor = cx.argument::<JsBoolean>(2)?.value(&mut cx);
    let keyboard = cx.argument::<JsBoolean>(3)?.value(&mut cx);

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id)
                .request(ListenInput { cursor, keyboard })
                .await?;

            Ok(())
        }),
    )
}

fn overlay_block_input(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let block = cx.argument::<JsBoolean>(2)?.value(&mut cx);

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.window(win_id).request(BlockInput { block }).await?;

            Ok(())
        }),
    )
}

fn overlay_set_blocking_cursor(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let win_id = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
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
            conn.window(win_id)
                .request(SetBlockingCursor { cursor })
                .await?;

            Ok(())
        }),
    )
}

pub fn export_module_functions(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("attach", attach)?;

    cx.export_function("overlaySetPosition", overlay_set_position)?;
    cx.export_function("overlaySetAnchor", overlay_set_anchor)?;
    cx.export_function("overlaySetMargin", overlay_set_margin)?;
    cx.export_function("overlayUpdateHandle", overlay_update_handle)?;
    cx.export_function("overlayListenInput", overlay_listen_input)?;
    cx.export_function("overlayBlockInput", overlay_block_input)?;
    cx.export_function("overlaySetBlockingCursor", overlay_set_blocking_cursor)?;

    cx.export_function("overlayCallNextEvent", overlay_call_next_event)?;

    cx.export_function("overlayDestroy", overlay_destroy)?;
    Ok(())
}
