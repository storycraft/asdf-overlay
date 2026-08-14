mod io;

use anyhow::Context;
use asdf_overlay::{
    event_sink::OverlayEventSink,
    surface::{SharedTextureHandle, Surfaces},
};
use asdf_overlay_common::{
    event::{OverlayEvent, surface::SurfaceEvent, window::WindowEvent},
    request::{
        BlockInput, Request, Requestable, SetBlockingCursor,
        surface::{
            SetPosition, SurfaceRequest, SurfaceRequestKind, SurfaceRequestable, UpdateSharedHandle,
        },
        window::{ListenInput, WindowRequest, WindowRequestKind, WindowRequestable},
    },
};
use asdf_overlay_window::{Backends, window::ListenInputFlags};
use scopeguard::defer;
use std::sync::Arc;
use tokio::net::windows::named_pipe::NamedPipeServer;
use tracing::{Level, debug, trace};

use crate::{cursors, event_sink::EventSink, ipc::io::IpcServerConn};

/// IPC server main loop.
#[tracing::instrument(level = Level::DEBUG, skip(backends, server))]
pub async fn run(
    hinstance: usize,
    backends: Arc<Backends>,
    server: NamedPipeServer,
) -> anyhow::Result<()> {
    let mut conn = IpcServerConn::new(server).await?;
    let emitter = conn.create_emitter();
    {
        debug!("sending initial data");
        // send existing windows
        for id in backends.windows() {
            backends.window(id, |state| {
                let (width, height) = state.size();

                _ = emitter.emit(OverlayEvent::Window {
                    id,
                    event: WindowEvent::Added { width, height },
                });
            });
        }

        // send existing surfaces
        for id in Surfaces::iter() {
            Surfaces::state(id, |state| {
                let (width, height) = state.size();

                _ = emitter.emit(OverlayEvent::Surface {
                    id,
                    event: SurfaceEvent::Added {
                        width,
                        height,
                        info: state.info,
                    },
                });
            });
        }
    }

    // setup event sink
    EventSink::set(move |event| {
        _ = emitter.emit(event);
    });

    // setup overlay event sink
    OverlayEventSink::set({
        use asdf_overlay_common::event::surface::Event;

        move |event| match event {
            Event::Surface { id, event } => {
                EventSink::emit(OverlayEvent::Surface { id, event });
            }
        }
    });

    defer!({
        debug!("cleanup start");
        EventSink::clear();
        OverlayEventSink::clear();
        backends.reset();
        Surfaces::reset();
    });

    while let Ok((req_id, req)) = conn.recv().await {
        trace!("recv id: {req_id} req: {req:?}");

        match req {
            Request::Window(window) => {
                handle_window_request(&mut conn, req_id, &backends, window)?;
            }

            Request::BlockInput(BlockInput { block }) => {
                conn.reply_with::<<BlockInput as Requestable>::Response>(req_id, || {
                    if block {
                        backends.block_input();
                    } else {
                        backends.unblock_input();
                    }

                    Ok(())
                })?;
            }

            Request::SetBlockingCursor(SetBlockingCursor { cursor }) => {
                conn.reply_with::<<SetBlockingCursor as Requestable>::Response>(req_id, || {
                    backends.set_blocking_cursor(
                        cursor.and_then(|cursor| cursors::load(hinstance, cursor)),
                    );

                    Ok(())
                })?;
            }

            Request::Surface(surface) => {
                handle_surface_request(&mut conn, req_id, surface)?;
            }
        }
    }
    Ok(())
}

fn handle_window_request(
    conn: &mut IpcServerConn,
    req_id: u32,
    backends: &Backends,
    req: WindowRequest,
) -> anyhow::Result<()> {
    match req.kind {
        WindowRequestKind::ListenInput(cmd) => {
            conn.reply_with::<<ListenInput as WindowRequestable>::Response>(req_id, || {
                let mut flags = ListenInputFlags::empty();
                flags.set(ListenInputFlags::CURSOR, cmd.cursor);
                flags.set(ListenInputFlags::KEYBOARD, cmd.keyboard);

                backends.window(req.id, |state| state.set_input_flags(flags));

                Ok(())
            })?;
        }
    }

    Ok(())
}

fn handle_surface_request(
    conn: &mut IpcServerConn,
    req_id: u32,
    req: SurfaceRequest,
) -> anyhow::Result<()> {
    match req.kind {
        SurfaceRequestKind::SetPosition(cmd) => {
            conn.reply_with::<<SetPosition as SurfaceRequestable>::Response>(req_id, || {
                Surfaces::state(req.id, |state| state.reposition(cmd.x, cmd.y))
                    .context("Surface not found")?;
                Ok(())
            })?;
        }

        SurfaceRequestKind::UpdateSharedHandle(shared) => {
            conn.reply_with::<<UpdateSharedHandle as SurfaceRequestable>::Response>(
                req_id,
                || {
                    Surfaces::state(req.id, |state| {
                        state
                            .commit_overlay_texture(map_ipc_shtex_update(shared))
                            .context("Failed to commit overlay texture")
                    })
                    .context("Surface not found")??;

                    Ok(())
                },
            )?;
        }
    }

    Ok(())
}

fn map_ipc_shtex_update(shared: UpdateSharedHandle) -> Option<SharedTextureHandle> {
    match shared {
        UpdateSharedHandle::Kmt(handle) => Some(SharedTextureHandle::Kmt(handle)),
        UpdateSharedHandle::Nt(handle) => Some(SharedTextureHandle::Nt(handle)),
        UpdateSharedHandle::None => None,
    }
}
