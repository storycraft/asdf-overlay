use core::time::Duration;
use std::path::PathBuf;

use crate::event::input::Cursor;
use crate::event::{create_emit_tsfn, event_task};
use crate::surface::UpdateSharedHandle;
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::client::IpcClientEventStream;
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
use tokio::runtime::Handle;

#[napi(custom_finalize)]
pub struct Overlay {
    ipc: Option<tokio::sync::Mutex<IpcClientConn>>,
    emitter_ref: Mutex<ObjectRef>,
}

#[napi]
impl Overlay {
    /// Inject the architecture-specific DLL and resolve a promise with its IPC overlay.
    ///
    /// The directory must contain the matching `asdf_overlay-x64.dll`,
    /// `asdf_overlay-x86.dll`, or `asdf_overlay-aarch64.dll`. Prefer an absolute
    /// directory accessible to the target. The optional timeout is in milliseconds;
    /// native wait value `u32::MAX` means infinite. See [`inject`] for blocking,
    /// timeout, architecture, and partial-failure caveats.
    ///
    /// JavaScript emitter setup can fail immediately; injection/connection failures
    /// reject the returned promise.
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

        let task = async move {
            let timeout = timeout.map(|timeout| Duration::from_millis(timeout as _));
            let handle = Handle::current();
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

            Ok((ipc, event, handle))
        };

        let cb = move |env, (ipc, event, handle): (IpcClientConn, IpcClientEventStream, Handle)| {
            let emit_tsfn = create_emit_tsfn(&emitter_ref.get_value(env)?)?;
            handle.spawn(event_task(event, emit_tsfn));

            Ok(Self {
                ipc: Some(ipc.into()),
                emitter_ref: Mutex::new(emitter_ref),
            })
        };

        Ok(env.spawn_future_with_callback(task, cb)?)
    }

    async fn ipc(&self) -> anyhow::Result<tokio::sync::MutexGuard<'_, IpcClientConn>> {
        Ok(self
            .ipc
            .as_ref()
            .context("Overlay is detached")?
            .lock()
            .await)
    }

    /// Return the existing JavaScript event emitter, including after detachment.
    ///
    /// This does not reconnect or replay past events. Errors accessing the stored
    /// JavaScript reference are returned.
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

    /// Send a shared-texture replacement or removal request for a surface ID.
    ///
    /// Use a nonnegative ID representable in `u64` from a surface event. Sign and
    /// lossless-conversion flags are ignored when extracting the BigInt, so invalid
    /// values can address an unintended surface. NT handles must already be valid
    /// in the target process; IPC does not duplicate them.
    ///
    /// Returns detached/transport/server errors, including unknown surface IDs.
    /// Success acknowledges the update without waiting for presentation.
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

    /// Set a surface's overlay offset in physical pixels without moving the host window.
    ///
    /// Negative and off-surface offsets are accepted. The BigInt ID has the same
    /// conversion restrictions as [`Self::update_handle`]. Detached connections,
    /// transport failures, and unknown surfaces return errors.
    #[napi]
    pub async fn set_position(&self, id: BigInt, x: i32, y: i32) -> anyhow::Result<()> {
        self.surface_request(id, SetPosition { x, y }).await?;

        Ok(())
    }

    /// Set the cursor used during process-wide blocking, or hide it with `None`.
    ///
    /// Invalid cursor values and request failures return errors. Success stores
    /// the choice but does not wait for a cursor update on the target's threads.
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

    /// Replace a window's cursor/keyboard listening flags without enabling blocking.
    ///
    /// Use a window ID, not a surface ID. An unknown window currently succeeds
    /// without effect. Global blocking captures input regardless of these flags.
    /// Detached connections and request failures return errors.
    #[napi]
    pub async fn listen_input(&self, id: u32, cursor: bool, keyboard: bool) -> anyhow::Result<()> {
        self.window_request(id, ListenInput { cursor, keyboard })
            .await?;

        Ok(())
    }

    /// Enable or disable input blocking across the target process's intercepted windows.
    ///
    /// Repeating the current state does nothing. Success does not wait for queued
    /// cursor/IME changes. Detached connections and request failures return errors.
    #[napi]
    pub async fn block_input(&self, block: bool) -> anyhow::Result<()> {
        self.request(BlockInput { block }).await?;

        Ok(())
    }

    /// Drop the IPC connection and stop its background reader.
    ///
    /// This does not unload the injected DLL or synchronously wait for server
    /// cleanup. The event emitter remains accessible. Returns an error if already
    /// detached; subsequent request methods also fail.
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

fn create_event_emitter<'env>(overlay: This<'env>) -> anyhow::Result<Object<'env>> {
    // See index.js
    let event_emitter_ctor = overlay.get_named_property::<Function<(), Object>>("EventEmitter")?;
    Ok(event_emitter_ctor.new_instance(())?.coerce_to_object()?)
}
