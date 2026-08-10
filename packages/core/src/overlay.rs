use core::time::Duration;
use std::path::PathBuf;

use crate::event::input::Cursor;
use crate::event::{create_emit_tsfn, event_task};
use crate::surface::UpdateSharedHandle;
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::common;
use asdf_overlay_client::common::request::Requestable;
use asdf_overlay_client::common::request::surface::{self, SetPosition, SurfaceRequestable};
use asdf_overlay_client::common::request::window::WindowRequestable;
use asdf_overlay_client::{
    OverlayDll,
    client::IpcClientConn,
    common::request::{BlockInput, SetBlockingCursor, window::ListenInput},
    inject,
};
use napi::bindgen_prelude::{
    BigInt, Function, JsObjectValue, Object, ObjectFinalize, ObjectRef, PromiseRaw, This,
};
use napi::{Env, JsValue};
use napi_derive::napi;
use num::FromPrimitive;
use parking_lot::Mutex;

#[napi(custom_finalize)]
pub struct Overlay {
    ipc: Option<tokio::sync::Mutex<IpcClientConn>>,
    emitter_ref: Mutex<ObjectRef>,
}

#[napi]
impl Overlay {
    /// Attach overlay to target process
    #[napi]
    pub fn attach<'env>(
        env: &'env Env,
        this: This,
        dll_dir: PathBuf,
        pid: u32,
        timeout: Option<u32>,
        // Self is not used due to bug in napi-rs generated typing
    ) -> anyhow::Result<PromiseRaw<'env, Overlay>> {
        let emitter = create_event_emitter(this)?;
        let emitter_ref = emitter.create_ref()?;
        let emit_tsfn = create_emit_tsfn(&emitter)?;

        Ok(env.spawn_future(async move {
            let timeout = timeout.map(|timeout| Duration::from_millis(timeout as _));
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

            tokio::spawn(event_task(event, emit_tsfn));
            Ok(Self {
                ipc: Some(ipc.into()),
                emitter_ref: Mutex::new(emitter_ref),
            })
        })?)
    }

    async fn ipc(&self) -> anyhow::Result<tokio::sync::MutexGuard<'_, IpcClientConn>> {
        Ok(self
            .ipc
            .as_ref()
            .context("Overlay is detached")?
            .lock()
            .await)
    }

    #[napi(getter, ts_return_type = "OverlayEventEmitter")]
    pub fn event<'env>(&self, env: &'env Env) -> anyhow::Result<Object<'env>> {
        Ok(self.emitter_ref.lock().get_value(env)?)
    }

    async fn request<T: Requestable>(&self, request: T) -> anyhow::Result<T::Response> {
        self.ipc().await?.request(request).await
    }

    async fn window_request<T: WindowRequestable>(
        &self,
        id: u32,
        request: T,
    ) -> anyhow::Result<T::Response> {
        self.ipc().await?.window(id).request(request).await
    }

    async fn surface_request<T: SurfaceRequestable>(
        &self,
        id: BigInt,
        request: T,
    ) -> anyhow::Result<T::Response> {
        self.ipc()
            .await?
            .surface(id.get_u64().1)
            .request(request)
            .await
    }

    /// Update overlay surface.
    #[napi]
    pub async fn update_handle(
        &self,
        id: BigInt,
        update: UpdateSharedHandle,
    ) -> anyhow::Result<()> {
        self.surface_request(id, Into::<surface::UpdateSharedHandle>::into(update))
            .await?;

        Ok(())
    }

    /// Update overlay position relative to window
    #[napi]
    pub async fn set_position(&self, id: BigInt, x: i32, y: i32) -> anyhow::Result<()> {
        self.surface_request(id, SetPosition { x, y }).await?;

        Ok(())
    }

    /// Set blocking cursor.
    #[napi]
    pub async fn set_blocking_cursor(&self, cursor: Option<Cursor>) -> anyhow::Result<()> {
        let cursor = cursor
            .map(|cursor| {
                common::cursor::Cursor::from_u32(cursor as _).context("invalid cursor value")
            })
            .transpose()?;

        self.request(SetBlockingCursor { cursor }).await?;
        Ok(())
    }

    /// Listen to window input without blocking
    #[napi]
    pub async fn listen_input(&self, id: u32, cursor: bool, keyboard: bool) -> anyhow::Result<()> {
        self.window_request(id, ListenInput { cursor, keyboard })
            .await?;

        Ok(())
    }

    /// Block window input and listen them.
    #[napi]
    pub async fn block_input(&self, block: bool) -> anyhow::Result<()> {
        self.request(BlockInput { block }).await?;

        Ok(())
    }

    /// Detach and destroy overlay
    #[napi]
    pub fn detach(&mut self) -> anyhow::Result<()> {
        self.ipc.take().context("overlay is already detached")?;
        Ok(())
    }
}

impl ObjectFinalize for Overlay {
    fn finalize(self, env: Env) -> napi::Result<()> {
        self.emitter_ref.into_inner().unref(&env)?;
        Ok(())
    }
}

fn create_event_emitter<'env>(overlay: This) -> anyhow::Result<Object<'env>> {
    // See index.js
    let event_emitter_ctor = overlay.get_named_property::<Function<(), Object>>("EventEmitter")?;
    Ok(event_emitter_ctor.new_instance(())?.coerce_to_object()?)
}
