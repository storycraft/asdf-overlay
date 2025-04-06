use core::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::sync::LazyLock;

use anyhow::Context as AnyhowContext;
use asdf_overlay_client::prelude::*;
use dashmap::DashMap;
use neon::{prelude::*, types::buffer::TypedArray};
use once_cell::sync::OnceCell;
use rustc_hash::FxBuildHasher;
use tokio::runtime::Runtime;

struct Manager {
    next_id: AtomicU32,
    map: DashMap<u32, tokio::sync::Mutex<IpcServerConn>, FxBuildHasher>,
}

impl Manager {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(0),
            map: DashMap::with_hasher(FxBuildHasher),
        }
    }

    async fn attach(&self, name: &str, timeout: Option<Duration>) -> anyhow::Result<u32> {
        let id = self.next_id.fetch_add(1, Ordering::AcqRel);

        let process = OwnedProcess::find_first_by_name(name)
            .with_context(|| format!("cannot find process: {name}"))?;
        let conn = inject(process, None, timeout)
            .await
            .context("cannot inject to the process")?;
        self.map.insert(id, tokio::sync::Mutex::new(conn));

        Ok(id)
    }

    async fn reposition(&self, id: u32, x: f32, y: f32) -> anyhow::Result<()> {
        let conn = self.map.get(&id).context("invalid id")?;
        conn.lock()
            .await
            .request(&Request::Position(UpdatePosition { x, y }))
            .await?;

        Ok(())
    }

    async fn update_bitmap(&self, id: u32, width: u32, data: Vec<u8>) -> anyhow::Result<()> {
        let conn = self.map.get(&id).context("invalid id")?;
        conn.lock()
            .await
            .request(&Request::Bitmap(UpdateBitmap { width, data }))
            .await?;

        Ok(())
    }

    async fn destroy(&self, id: u32) -> anyhow::Result<bool> {
        let (_, conn) = self.map.remove(&id).context("invalid id")?;
        if conn.into_inner().request(&Request::Close).await.is_ok() {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

static MANAGER: LazyLock<Manager> = LazyLock::new(Manager::new);

fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(format!("{err:?}"))))
}

fn attach(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let name = cx.argument::<JsString>(0)?.value(&mut cx);
    let timeout = cx
        .argument_opt(1)
        .filter(|v| !v.is_a::<JsUndefined, _>(&mut cx))
        .map(|v| v.downcast_or_throw::<JsNumber, _>(&mut cx))
        .transpose()?
        .map(|timeout| Duration::from_millis(timeout.value(&mut cx) as _));

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = MANAGER.attach(&name, timeout).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(id) => Ok(JsNumber::new(&mut cx, id)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_reposition(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let x = cx.argument::<JsNumber>(1)?.value(&mut cx) as f32;
    let y = cx.argument::<JsNumber>(2)?.value(&mut cx) as f32;

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = MANAGER.reposition(id, x, y).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(_) => Ok(JsUndefined::new(&mut cx)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_update_bitmap(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let data = cx.argument::<JsBuffer>(2)?.as_slice(&cx).to_vec();

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = MANAGER.update_bitmap(id, width, data).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(_) => Ok(JsUndefined::new(&mut cx)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

fn overlay_close(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();

    rt.spawn(async move {
        let res = MANAGER.destroy(id).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(ejected) => Ok(JsBoolean::new(&mut cx, ejected)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("attach", attach)?;
    cx.export_function("overlayUpdateBitmap", overlay_update_bitmap)?;
    cx.export_function("overlayReposition", overlay_reposition)?;
    cx.export_function("overlayClose", overlay_close)?;
    Ok(())
}
