mod util;
mod wrapper;

use core::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::{os::windows::io::AsRawHandle, path::PathBuf, sync::LazyLock};

use anyhow::{Context as AnyhowContext, bail};
use asdf_overlay_client::prelude::*;
use bytemuck::pod_read_unaligned;
use dashmap::DashMap;
use mimalloc::MiMalloc;
use neon::{prelude::*, types::buffer::TypedArray};
use once_cell::sync::OnceCell;
use rustc_hash::FxBuildHasher;
use tokio::runtime::Runtime;
use util::{get_process_arch, request_promise};
use windows::Win32::{
    Foundation::HANDLE,
    System::SystemInformation::{
        IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
    },
};
use wrapper::percent_length_from_object;

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

    async fn attach(
        &self,
        dll_dir: PathBuf,
        pid: u32,
        timeout: Option<Duration>,
    ) -> anyhow::Result<u32> {
        let process = OwnedProcess::from_pid(pid)
            .with_context(|| format!("cannot find process pid: {pid}"))?;

        let dll_path = match get_process_arch(HANDLE(process.as_raw_handle())) {
            IMAGE_FILE_MACHINE_AMD64 => "asdf_overlay-x64.dll",
            IMAGE_FILE_MACHINE_I386 => "asdf_overlay-x86.dll",
            IMAGE_FILE_MACHINE_ARM64 => "asdf_overlay-aarch64.dll",
            arch => bail!("Unsupported arch: {}", arch.0),
        };

        let conn = inject(
            process,
            Some({
                let mut dll = dll_dir;
                dll.push(dll_path);
                dll
            }),
            timeout,
        )
        .await
        .context("cannot inject to the process")?;

        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        self.map.insert(id, tokio::sync::Mutex::new(conn));

        Ok(id)
    }

    async fn request(&self, id: u32, request: Request) -> anyhow::Result<()> {
        let conn = self.map.get(&id).context("invalid id")?;
        conn.lock().await.request(request).await?;

        Ok(())
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
    let x = percent_length_from_object(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(2)?;
    let y = percent_length_from_object(&mut cx, &y)?;

    request_promise(&mut cx, id, Request::UpdatePosition(Position { x, y }))
}

fn overlay_set_anchor(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let x = cx.argument::<JsObject>(1)?;
    let x = percent_length_from_object(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(2)?;
    let y = percent_length_from_object(&mut cx, &y)?;

    request_promise(&mut cx, id, Request::UpdateAnchor(Anchor { x, y }))
}

fn overlay_set_margin(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let top = cx.argument::<JsObject>(1)?;
    let top = percent_length_from_object(&mut cx, &top)?;
    let right = cx.argument::<JsObject>(2)?;
    let right = percent_length_from_object(&mut cx, &right)?;
    let bottom = cx.argument::<JsObject>(3)?;
    let bottom = percent_length_from_object(&mut cx, &bottom)?;
    let left = cx.argument::<JsObject>(4)?;
    let left = percent_length_from_object(&mut cx, &left)?;

    request_promise(
        &mut cx,
        id,
        Request::UpdateMargin(Margin {
            top,
            right,
            bottom,
            left,
        }),
    )
}

fn overlay_update_bitmap(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let width = cx.argument::<JsNumber>(1)?.value(&mut cx) as u32;
    let data = cx.argument::<JsBuffer>(2)?.as_slice(&cx).to_vec();

    request_promise(&mut cx, id, Request::UpdateBitmap(Bitmap { width, data }))
}

fn overlay_update_shtex(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;
    let handle_slice = cx.argument::<JsBuffer>(1)?.as_slice(&mut cx);
    let handle = pod_read_unaligned::<usize>(handle_slice);

    request_promise(
        &mut cx,
        id,
        Request::UpdateShtex(SharedDx11Handle { handle }),
    )
}

fn overlay_destroy(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;

    match MANAGER.destroy(id) {
        Ok(_) => Ok(JsUndefined::new(&mut cx)),
        Err(err) => cx.throw_error(format!("{err:?}")),
    }
}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("attach", attach)?;

    cx.export_function("overlaySetPosition", overlay_set_position)?;
    cx.export_function("overlaySetAnchor", overlay_set_anchor)?;
    cx.export_function("overlaySetMargin", overlay_set_margin)?;

    cx.export_function("overlayUpdateBitmap", overlay_update_bitmap)?;
    cx.export_function("overlayUpdateShtex", overlay_update_shtex)?;

    cx.export_function("overlayDestroy", overlay_destroy)?;
    Ok(())
}
