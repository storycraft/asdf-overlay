use core::time::Duration;
use std::path::PathBuf;

use crate::PercentLength;
use crate::event::input::Cursor;
use crate::event::{create_emit_tsfn, event_task};
use crate::surface::UpdateSharedHandle;
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::common::request::WindowRequestItem;
use asdf_overlay_client::common::{self, request};
use asdf_overlay_client::{
    OverlayDll,
    client::IpcClientConn,
    common::request::{
        BlockInput, ListenInput, SetAnchor, SetBlockingCursor, SetMargin, SetPosition,
    },
    inject,
};
use napi::bindgen_prelude::{
    Function, JsObjectValue, Object, ObjectFinalize, ObjectRef, PromiseRaw, This,
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

    async fn window_request(&self, id: u32, request: impl WindowRequestItem) -> anyhow::Result<()> {
        self.ipc().await?.window(id).request(request).await?;
        Ok(())
    }

    /// Update overlay surface.
    #[napi]
    pub async fn update_handle(&self, id: u32, update: UpdateSharedHandle) -> anyhow::Result<()> {
        self.window_request(id, Into::<request::UpdateSharedHandle>::into(update))
            .await
    }

    /// Update overlay position relative to window
    #[napi]
    pub async fn set_position(
        &self,
        id: u32,
        x: PercentLength,
        y: PercentLength,
    ) -> anyhow::Result<()> {
        self.window_request(
            id,
            SetPosition {
                x: x.into(),
                y: y.into(),
            },
        )
        .await
    }

    /// Update overlay anchor
    #[napi]
    pub async fn set_anchor(
        &self,
        id: u32,
        x: PercentLength,
        y: PercentLength,
    ) -> anyhow::Result<()> {
        self.window_request(
            id,
            SetAnchor {
                x: x.into(),
                y: y.into(),
            },
        )
        .await
    }

    /// Update overlay margin
    #[napi]
    pub async fn set_margin(
        &self,
        id: u32,
        top: PercentLength,
        right: PercentLength,
        bottom: PercentLength,
        left: PercentLength,
    ) -> anyhow::Result<()> {
        self.window_request(
            id,
            SetMargin {
                top: top.into(),
                right: right.into(),
                bottom: bottom.into(),
                left: left.into(),
            },
        )
        .await
    }

    /// Set blocking cursor.
    #[napi]
    pub async fn set_blocking_cursor(&self, id: u32, cursor: Option<Cursor>) -> anyhow::Result<()> {
        let cursor = cursor
            .map(|cursor| {
                common::cursor::Cursor::from_u32(cursor as _).context("invalid cursor value")
            })
            .transpose()?;

        self.window_request(id, SetBlockingCursor { cursor }).await
    }

    /// Listen to window input without blocking
    #[napi]
    pub async fn listen_input(&self, id: u32, cursor: bool, keyboard: bool) -> anyhow::Result<()> {
        self.window_request(id, ListenInput { cursor, keyboard })
            .await
    }

    /// Block window input and listen them.
    #[napi]
    pub async fn block_input(&self, block: bool) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .request(request::Request::BlockInput(BlockInput { block }))
            .await?;

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
