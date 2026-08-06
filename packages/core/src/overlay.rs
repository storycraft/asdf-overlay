use core::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;

use crate::PercentLength;
use crate::input::Cursor;
use crate::surface::UpdateSharedHandle;
use anyhow::Context as AnyhowContext;
use asdf_overlay_client::common::{self, request};
use asdf_overlay_client::{
    OverlayDll,
    client::{IpcClientConn, IpcClientEventStream},
    common::request::{
        BlockInput, ListenInput, SetAnchor, SetBlockingCursor, SetMargin, SetPosition,
    },
    inject,
};
use napi::Env;
use napi::bindgen_prelude::{AsyncGenerator, Buffer, Reference};
use napi_derive::napi;
use num::FromPrimitive;
use tokio::sync::{Mutex, MutexGuard};

#[napi]
pub struct Overlay {
    ipc: Option<Mutex<IpcClientConn>>,
    events: OverlayEventStream,
}

#[napi]
impl Overlay {
    /// Attach overlay to target process
    #[napi(factory)]
    pub async fn attach(dll_dir: PathBuf, pid: u32, timeout: Option<u32>) -> anyhow::Result<Self> {
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

        Ok(Self {
            ipc: Some(ipc.into()),
            events: OverlayEventStream::new(event),
        })
    }

    async fn ipc(&self) -> anyhow::Result<MutexGuard<'_, IpcClientConn>> {
        Ok(self
            .ipc
            .as_ref()
            .context("Overlay is detached")?
            .lock()
            .await)
    }

    #[napi(getter)]
    pub fn events(&self) -> OverlayEventStream {
        self.events.clone()
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
        Ok(())
    }
}

#[napi(async_iterator)]
#[derive(Clone)]
pub struct OverlayEventStream {
    stream: Arc<Mutex<IpcClientEventStream>>,
}

#[napi]
impl AsyncGenerator for OverlayEventStream {
    type Yield = Buffer;
    type Next = ();
    type Return = ();

    fn next(
        &mut self,
        _: Option<()>,
    ) -> impl Future<Output = napi::Result<Option<Self::Yield>>> + Send + 'static {
        let stream = self.stream.clone();

        async move {
            let Some(event) = stream.lock().await.recv().await else {
                return Ok(None);
            };

            // TODO
            Ok(Some(Buffer::default()))
        }
    }
}

impl OverlayEventStream {
    pub fn new(stream: IpcClientEventStream) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
        }
    }
}