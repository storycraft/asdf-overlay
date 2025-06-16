use core::{mem, time::Duration};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_common::{
    event::{ClientEvent, WindowEvent},
    ipc::server::{IpcClientEventEmitter, IpcServerConn},
    request::{Request, WindowRequest},
};
use scopeguard::defer;
use tokio::{net::windows::named_pipe::NamedPipeServer, time::sleep};
use tracing::{debug, error, trace, warn};
use windows::Win32::Foundation::HWND;

use crate::backend::{Backends, ListenInputFlags};

static CONN: ArcSwapOption<OverlayIpc> = ArcSwapOption::const_empty();

#[derive(Clone)]
pub struct OverlayIpc {
    emitter: IpcClientEventEmitter,
}

impl OverlayIpc {
    #[inline]
    pub fn connected() -> bool {
        CONN.load().is_some()
    }

    #[inline]
    pub fn emit_event(event: ClientEvent) {
        if let Some(ref this) = *CONN.load() {
            _ = this.emitter.emit(event);
        }
    }
}

#[tracing::instrument(skip(server))]
async fn run(mut server: IpcServerConn) -> anyhow::Result<()> {
    fn handle_window_event(hwnd: u32, req: WindowRequest) -> anyhow::Result<bool> {
        let res = Backends::with_backend(HWND(hwnd as _), |backend| match req {
            WindowRequest::SetPosition(position) => {
                backend.layout.position = position;
            }

            WindowRequest::SetAnchor(anchor) => {
                backend.layout.anchor = anchor;
            }

            WindowRequest::SetMargin(margin) => {
                backend.layout.margin = margin;
            }

            WindowRequest::ListenInput(cmd) => {
                let mut flags = ListenInputFlags::empty();
                flags.set(ListenInputFlags::CURSOR, cmd.cursor);
                flags.set(ListenInputFlags::KEYBOARD, cmd.keyboard);

                backend.listen_input = flags;
            }

            WindowRequest::BlockInput(cmd) => {
                backend.block_input(cmd.block);
            }

            WindowRequest::SetBlockingCursor(cmd) => {
                backend.blocking_cursor = cmd.cursor;
            }

            WindowRequest::UpdateSharedHandle(shared) => {
                backend.pending_handle = Some(shared);
            }
        });

        Ok(res.is_some())
    }

    loop {
        let (id, req) = server.recv().await?;
        trace!("recv id: {id} req: {req:?}");

        match req {
            Request::Window { hwnd, request } => {
                server.reply(id, handle_window_event(hwnd, request)?)?;
            }
        }
    }
}

#[tracing::instrument(skip(create_server))]
pub async fn app(
    mut server: NamedPipeServer,
    mut create_server: impl FnMut() -> anyhow::Result<NamedPipeServer>,
) {
    async fn inner(server: NamedPipeServer) -> anyhow::Result<()> {
        let conn = IpcServerConn::new(server).await?;
        debug!("ipc client connected");

        debug!("sending initial data");
        let emitter = conn.create_emitter();
        // send existing windows
        for backend in Backends::iter() {
            _ = emitter.emit(ClientEvent::Window {
                hwnd: *backend.key() as _,
                event: WindowEvent::Added {
                    width: backend.size.0,
                    height: backend.size.1,
                },
            });
        }
        debug!("initial data sent");

        CONN.store(Some(Arc::new(OverlayIpc {
            emitter: emitter.clone(),
        })));

        defer!({
            debug!("cleanup start");
            Backends::cleanup_backends();
            CONN.store(None);
        });

        run(conn).await?;

        Ok(())
    }

    loop {
        debug!("waiting ipc client...");
        let res = server.connect().await;
        let new_server = loop {
            match create_server() {
                Ok(server) => break server,
                Err(err) => {
                    error!("failed to create server. retrying after 5 seconds. err: {err:?}");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        };

        match res {
            Ok(_) => {
                if let Err(err) = inner(mem::replace(&mut server, new_server)).await {
                    warn!("client connection ended unexpectedly. err: {:?}", err);
                }
            }
            Err(err) => {
                error!("failed to connect to client. err: {err:?}");
            }
        }
    }
}
