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

use crate::backend::{Backends, window::ListenInputFlags};

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
                backend
                    .proc
                    .lock()
                    .layout
                    .set_position(position.x, position.y);
                backend.recalc_position();
            }

            WindowRequest::SetAnchor(anchor) => {
                backend.proc.lock().layout.set_anchor(anchor.x, anchor.y);
                backend.recalc_position();
            }

            WindowRequest::SetMargin(margin) => {
                backend.proc.lock().layout.set_margin(
                    margin.top,
                    margin.right,
                    margin.bottom,
                    margin.left,
                );
                backend.recalc_position();
            }

            WindowRequest::ListenInput(cmd) => {
                let mut flags = ListenInputFlags::empty();
                flags.set(ListenInputFlags::CURSOR, cmd.cursor);
                flags.set(ListenInputFlags::KEYBOARD, cmd.keyboard);

                backend.proc.lock().listen_input = flags;
            }

            WindowRequest::BlockInput(cmd) => {
                backend.proc.lock().block_input(cmd.block, backend.hwnd);
            }

            WindowRequest::SetBlockingCursor(cmd) => {
                backend.proc.lock().blocking_cursor = cmd.cursor;
            }

            WindowRequest::UpdateSharedHandle(shared) => {
                let res = backend.render.lock().update_surface(shared.handle);
                match res {
                    Ok(_) => backend.recalc_position(),
                    Err(err) => error!("failed to open shared surface. err: {:?}", err),
                }
            }
        });

        Ok(res.is_some())
    }

    loop {
        let (req_id, req) = server.recv().await?;
        trace!("recv id: {req_id} req: {req:?}");

        match req {
            Request::Window { id, request } => {
                server.reply(req_id, handle_window_event(id, request)?)?;
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
            let size = backend.render.lock().window_size;
            _ = emitter.emit(ClientEvent::Window {
                id: *backend.key() as _,
                event: WindowEvent::Added {
                    width: size.0,
                    height: size.1,
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
