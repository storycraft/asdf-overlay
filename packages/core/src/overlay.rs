use core::time::Duration;
use std::path::PathBuf;

use crate::PercentLength;
use crate::event::input::Cursor;
use crate::event::{create_emit_tsfn, event_task};
use crate::surface::UpdateSharedHandle;
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::common::{self, request};
use asdf_overlay_client::{
    OverlayDll,
    client::IpcClientConn,
    common::request::{
        BlockInput, ListenInput, SetAnchor, SetBlockingCursor, SetMargin, SetPosition,
    },
    inject,
};
use napi::bindgen_prelude::{Function, JsObjectValue, Object, ObjectRef, PromiseRaw, This};
use napi::{Env, JsValue};
use napi_derive::napi;
use num::FromPrimitive;
use parking_lot::Mutex;

#[napi]
pub struct Overlay {
    ipc: Option<tokio::sync::Mutex<IpcClientConn>>,
    emitter_ref: Option<Mutex<ObjectRef>>,
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
                emitter_ref: Some(Mutex::new(emitter_ref)),
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
        Ok(self
            .emitter_ref
            .as_ref()
            .context("Overlay is detached")?
            .lock()
            .get_value(env)?)
    }

    /// Update overlay surface.
    #[napi]
    pub async fn update_handle(&self, id: u32, update: UpdateSharedHandle) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .window(id)
            .request(Into::<request::UpdateSharedHandle>::into(update))
            .await?;
        Ok(())
    }

    /// Update overlay position relative to window
    #[napi]
    pub async fn set_position(
        &self,
        id: u32,
        x: PercentLength,
        y: PercentLength,
    ) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .window(id)
            .request(SetPosition {
                x: x.into(),
                y: y.into(),
            })
            .await?;
        Ok(())
    }

    /// Update overlay anchor
    #[napi]
    pub async fn set_anchor(
        &self,
        id: u32,
        x: PercentLength,
        y: PercentLength,
    ) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .window(id)
            .request(SetAnchor {
                x: x.into(),
                y: y.into(),
            })
            .await?;
        Ok(())
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
        self.ipc()
            .await?
            .window(id)
            .request(SetMargin {
                top: top.into(),
                right: right.into(),
                bottom: bottom.into(),
                left: left.into(),
            })
            .await?;
        Ok(())
    }

    /// Set blocking cursor.
    #[napi]
    pub async fn set_blocking_cursor(&self, id: u32, cursor: Option<Cursor>) -> anyhow::Result<()> {
        let cursor = cursor
            .map(|cursor| {
                common::cursor::Cursor::from_u32(cursor as _).context("invalid cursor value")
            })
            .transpose()?;

        self.ipc()
            .await?
            .window(id)
            .request(SetBlockingCursor { cursor })
            .await?;
        Ok(())
    }

    /// Listen to window input without blocking
    #[napi]
    pub async fn listen_input(&self, id: u32, cursor: bool, keyboard: bool) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .window(id)
            .request(ListenInput { cursor, keyboard })
            .await?;
        Ok(())
    }

    /// Block window input and listen them.
    #[napi]
    pub async fn block_input(&self, id: u32, block: bool) -> anyhow::Result<()> {
        self.ipc()
            .await?
            .window(id)
            .request(BlockInput { block })
            .await?;
        Ok(())
    }

    /// Detach and destroy overlay
    #[napi]
    pub fn detach(&mut self, env: Env) -> anyhow::Result<()> {
        self.ipc.take().context("overlay is already detached")?;
        if let Some(r) = self.emitter_ref.take() {
            r.into_inner().unref(&env)?;
        }

        Ok(())
    }
}

fn create_event_emitter<'env>(overlay: This) -> anyhow::Result<Object<'env>> {
    // See index.js
    let event_emitter_ctor = overlay.get_named_property::<Function<(), Object>>("EventEmitter")?;
    Ok(event_emitter_ctor.new_instance(())?.coerce_to_object()?)
}
