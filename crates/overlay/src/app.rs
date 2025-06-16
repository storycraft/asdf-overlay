use core::{mem, time::Duration};

use asdf_overlay_common::{
    event::{ClientEvent, WindowEvent},
    ipc::server::{IpcClientEventEmitter, IpcServerConn},
    request::{Request, SetAnchor, SetMargin, SetPosition},
    size::PercentLength,
};
use parking_lot::RwLock;
use scopeguard::defer;
use tokio::{net::windows::named_pipe::NamedPipeServer, time::sleep};
use tracing::{debug, error, trace, warn};
use windows::Win32::Foundation::HWND;

use crate::backend::{Backends, ListenInputFlags};

static CURRENT: RwLock<OverlayState> = RwLock::new(OverlayState::Disabled);

enum OverlayState {
    Disabled,
    Enabled(Overlay),
}

pub struct Overlay {
    emitter: IpcClientEventEmitter,
    position: SetPosition,
    anchor: SetAnchor,
    margin: SetMargin,
}

impl Overlay {
    pub fn calc_overlay_position(&self, size: (f32, f32), screen: (u32, u32)) -> (f32, f32) {
        let screen = (screen.0 as f32, screen.1 as f32);

        let margin_left = self.margin.left.resolve(screen.0);
        let margin_top = self.margin.top.resolve(screen.1);

        let outer_width = margin_left + size.0 + self.margin.right.resolve(screen.0);
        let outer_height = margin_top + size.1 + self.margin.bottom.resolve(screen.1);

        let x =
            self.position.x.resolve(screen.0) - self.anchor.x.resolve(outer_width) + margin_left;
        let y =
            self.position.y.resolve(screen.1) - self.anchor.y.resolve(outer_height) + margin_top;

        (x, y)
    }

    #[inline]
    pub fn emit_event(event: ClientEvent) {
        _ = Overlay::with(|overlay| {
            _ = overlay.emitter.emit(event);
        });
    }

    #[must_use]
    #[inline]
    pub fn with<R>(f: impl FnOnce(&Self) -> R) -> Option<R> {
        match *CURRENT.read() {
            OverlayState::Disabled => None,
            OverlayState::Enabled(ref this) => Some(f(this)),
        }
    }

    #[inline]
    pub fn with_mut<R>(f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        match *CURRENT.write() {
            OverlayState::Disabled => None,
            OverlayState::Enabled(ref mut this) => Some(f(this)),
        }
    }
}

#[tracing::instrument(skip(server))]
async fn run(mut server: IpcServerConn) -> anyhow::Result<()> {
    loop {
        let (id, req) = server.recv().await?;
        trace!("recv id: {id} req: {req:?}");

        match req {
            Request::SetPosition(position) => {
                Overlay::with_mut(|overlay| overlay.position = position);
                server.reply(id, ())?;
            }

            Request::SetAnchor(anchor) => {
                Overlay::with_mut(|overlay| overlay.anchor = anchor);
                server.reply(id, ())?;
            }

            Request::SetMargin(margin) => {
                Overlay::with_mut(|overlay| overlay.margin = margin);
                server.reply(id, ())?;
            }

            Request::ListenInput(cmd) => {
                server.reply(
                    id,
                    Backends::with_backend(HWND(cmd.hwnd as _), |backend| {
                        let mut flags = ListenInputFlags::empty();
                        flags.set(ListenInputFlags::CURSOR, cmd.cursor);
                        flags.set(ListenInputFlags::KEYBOARD, cmd.keyboard);

                        backend.listen_input = flags;
                    })
                    .is_some(),
                )?;
            }

            Request::BlockInput(cmd) => {
                server.reply(
                    id,
                    Backends::with_backend(HWND(cmd.hwnd as _), |backend| {
                        backend.block_input(cmd.block);
                    })
                    .is_some(),
                )?;
            }

            Request::SetBlockingCursor(cmd) => {
                server.reply(
                    id,
                    Backends::with_backend(HWND(cmd.hwnd as _), |backend| {
                        backend.blocking_cursor = cmd.cursor;
                    })
                    .is_some(),
                )?;
            }

            Request::UpdateSharedHandle(shared) => {
                for mut backend in Backends::iter_mut() {
                    backend.pending_handle = Some(shared.clone());
                }

                server.reply(id, ())?;
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

        *CURRENT.write() = OverlayState::Enabled(Overlay {
            emitter,
            position: SetPosition {
                x: PercentLength::ZERO,
                y: PercentLength::ZERO,
            },
            anchor: SetAnchor {
                x: PercentLength::ZERO,
                y: PercentLength::ZERO,
            },
            margin: SetMargin {
                top: PercentLength::ZERO,
                right: PercentLength::ZERO,
                bottom: PercentLength::ZERO,
                left: PercentLength::ZERO,
            },
        });

        defer!({
            debug!("cleanup start");
            Backends::cleanup_backends();
            *CURRENT.write() = OverlayState::Disabled;
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
