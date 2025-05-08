mod event;
mod util;
mod wrapper;

use core::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::{os::windows::io::AsRawHandle, path::PathBuf, sync::LazyLock};

use anyhow::{Context as AnyhowContext, bail};
use asdf_overlay_client::{
    common::{
        event::ClientEvent,
        ipc::server::{IpcServerConn, IpcServerEventStream},
        request::{SetAnchor, SetMargin, SetPosition, UpdateSharedHandle},
    },
    inject,
    process::OwnedProcess,
    surface::OverlaySurface,
};
use bytemuck::pod_read_unaligned;
use dashmap::DashMap;
use event::serialize_event;
use futures::StreamExt;
use mimalloc::MiMalloc;
use neon::{prelude::*, types::buffer::TypedArray};
use once_cell::sync::OnceCell;
use rustc_hash::FxBuildHasher;
use tokio::runtime::Runtime;
use util::{get_process_arch, try_with_ipc, with_rt};
use windows::Win32::{
    Foundation::HANDLE,
    System::SystemInformation::{
        IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
    },
};
use wrapper::percent_length_from_object;

struct Overlay {
    surface: tokio::sync::Mutex<OverlaySurface>,
    ipc: tokio::sync::Mutex<IpcServerConn>,
    event: tokio::sync::Mutex<IpcServerEventStream>,
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
        name: String,
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

        let surface = OverlaySurface::new().context("cannot create dx11 device")?;
        let (ipc, stream) = inject(
            name,
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
    let name = cx.argument::<JsString>(0)?.value(&mut cx);
    let dll_dir = cx.argument::<JsString>(1)?.value(&mut cx);
    let pid = cx.argument::<JsNumber>(2)?.value(&mut cx) as u32;
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
        let res = MANAGER
            .attach(name, PathBuf::from(dll_dir), pid, timeout)
            .await;

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
    let x = percent_length_from_object(&mut cx, &x)?;
    let y = cx.argument::<JsObject>(2)?;
    let y = percent_length_from_object(&mut cx, &y)?;

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
    let top = percent_length_from_object(&mut cx, &top)?;
    let right = cx.argument::<JsObject>(2)?;
    let right = percent_length_from_object(&mut cx, &right)?;
    let bottom = cx.argument::<JsObject>(3)?;
    let bottom = percent_length_from_object(&mut cx, &bottom)?;
    let left = cx.argument::<JsObject>(4)?;
    let left = percent_length_from_object(&mut cx, &left)?;

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
    let handle_slice = cx.argument::<JsBuffer>(1)?.as_slice(&cx);
    let handle = pod_read_unaligned::<usize>(handle_slice);

    with_rt(&mut cx, async move {
        MANAGER
            .with(id, async move |overlay| {
                if let Some(shared) = overlay
                    .surface
                    .lock()
                    .await
                    .update_from_nt_shared(HANDLE(handle as _))?
                {
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

    with_rt(
        &mut cx,
        try_with_ipc(id, async move |conn| {
            conn.update_shtex(UpdateSharedHandle { handle: None })
                .await?;

            Ok(())
        }),
    )
}

fn overlay_next_event(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u32;

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = async move {
            let event: ClientEvent = MANAGER
                .with(id, async move |overlay| {
                    overlay
                        .event
                        .lock()
                        .await
                        .next()
                        .await
                        .context("event stream closed")
                })
                .await??;
            Ok::<_, anyhow::Error>(event)
        }
        .await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(event) => Ok(serialize_event(&mut cx, event)?),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
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
    cx.export_function("overlayClearSurface", overlay_clear_surface)?;

    cx.export_function("overlayNextEvent", overlay_next_event)?;

    cx.export_function("overlayDestroy", overlay_destroy)?;
    Ok(())
}
