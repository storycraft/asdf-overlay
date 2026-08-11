#![windows_subsystem = "windows"]

//! Official DLL crate for attaching [`asdf_overlay`] to other processes.
//! Using this DLL, the overlay can be controlled via cross-process IPC.
//!
//! Injection can be done using `asdf-overlay-client` crate.

mod cursors;
mod event_sink;
mod ipc_tracing;
mod server;

extern crate asdf_overlay_vulkan_layer;

use anyhow::Context;
use asdf_overlay::{event_sink::OverlayEventSink, initialize, surface::Surfaces};
use asdf_overlay_common::{
    event::{OverlayEvent, surface::SurfaceEvent, window::WindowEvent},
    ipc::create_ipc_addr,
    request::{
        BlockInput, Request, Requestable, SetBlockingCursor,
        surface::{
            SetPosition, SurfaceRequest, SurfaceRequestKind, SurfaceRequestable, UpdateSharedHandle,
        },
        window::{ListenInput, WindowRequest, WindowRequestKind, WindowRequestable},
    },
};
use asdf_overlay_window::{Backends, window::ListenInputFlags};
use core::time::Duration;
use scopeguard::defer;
use std::{ffi::OsStr, sync::Arc, thread};
use tokio::{
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    runtime::Runtime,
    time::sleep,
};
use tracing::{debug, error, trace, warn};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HINSTANCE},
        Security::{
            ACL, AllocateAndInitializeSid,
            Authorization::{
                EXPLICIT_ACCESS_A, SET_ACCESS, SetEntriesInAclA, TRUSTEE_A, TRUSTEE_IS_SID,
                TRUSTEE_IS_USER,
            },
            FreeSid, InitializeSecurityDescriptor, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
            SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_WORLD_SID_AUTHORITY,
            SetSecurityDescriptorDacl,
        },
        System::{
            SystemServices::{
                DLL_PROCESS_ATTACH, SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID,
            },
            Threading::GetCurrentProcessId,
        },
    },
    core::{BOOL, PSTR},
};

use crate::{event_sink::EventSink, server::IpcServerConn};

/// IPC server main loop.
#[tracing::instrument(skip(backends, server))]
async fn run(
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
                let gpu_id = state.interop.gpu_id();

                _ = emitter.emit(OverlayEvent::Surface {
                    id,
                    event: SurfaceEvent::Added {
                        width,
                        height,
                        gpu_id,
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
                if block {
                    backends.block_input();
                } else {
                    backends.unblock_input();
                }

                conn.reply::<<BlockInput as Requestable>::Response>(req_id, ())?;
            }

            Request::SetBlockingCursor(SetBlockingCursor { cursor }) => {
                backends.set_blocking_cursor(
                    cursor.and_then(|cursor| cursors::load(hinstance, cursor)),
                );

                conn.reply::<<SetBlockingCursor as Requestable>::Response>(req_id, ())?;
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
            let mut flags = ListenInputFlags::empty();
            flags.set(ListenInputFlags::CURSOR, cmd.cursor);
            flags.set(ListenInputFlags::KEYBOARD, cmd.keyboard);

            backends.window(req.id, |state| state.set_input_flags(flags));
            conn.reply::<<ListenInput as WindowRequestable>::Response>(req_id, ())?;
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
            _ = Surfaces::state(req.id, |state| state.set_position(cmd.x, cmd.y));
            conn.reply::<<SetPosition as SurfaceRequestable>::Response>(req_id, ())?;
        }

        SurfaceRequestKind::UpdateSharedHandle(shared) => {
            defer!({
                if let UpdateSharedHandle::Nt(handle) = shared {
                    _ = unsafe { CloseHandle(HANDLE(handle as _)) };
                }
            });
            Surfaces::state(req.id, |state| {
                if let Err(err) = state.set_overlay_texture(shared.handle()) {
                    error!("failed to open shared surface. err: {:?}", err);
                }
            });

            conn.reply::<<UpdateSharedHandle as SurfaceRequestable>::Response>(req_id, ())?;
        }
    }

    Ok(())
}

/// IPC server listener.
#[tracing::instrument(skip(create_server))]
async fn run_server(
    hinstance: usize,
    mut server: NamedPipeServer,
    mut create_server: impl FnMut() -> anyhow::Result<NamedPipeServer>,
) {
    // initialize windows
    let backends = Arc::new(Backends::new().expect("failed to setup backends"));

    // setup window event sink
    tokio::spawn({
        use asdf_overlay_common::event::window::Event;

        let backends = backends.clone();

        async move {
            while let Some(event) = backends.recv_async().await {
                EventSink::emit(match event {
                    Event::Window { id, event } => OverlayEvent::Window { id, event },
                    Event::InputBlockingEnded => OverlayEvent::InputBlockingEnded,
                });
            }
        }
    });

    // initialize overlay
    initialize().expect("initialization failed");
    debug!("hook installed");

    loop {
        debug!("waiting ipc client...");
        match server.connect().await {
            Ok(_) => {
                if let Err(err) = run(hinstance, backends.clone(), server).await {
                    warn!("client connection ended unexpectedly. err: {:?}", err);
                }
            }
            Err(err) => {
                error!("failed to connect to client. err: {err:?}");
            }
        }

        server = loop {
            match create_server() {
                Ok(server) => break server,
                Err(err) => {
                    error!("failed to create server. retrying after 5 seconds. err: {err:?}");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        };
    }
}

/// Main entry point for DLL.
///
/// # Safety
/// Can be called by loader only. Must not be called manually.
#[unsafe(no_mangle)]
#[allow(non_snake_case, unused_variables)]
pub unsafe extern "system" fn DllMain(dll_module: HINSTANCE, fdw_reason: u32, _: *mut ()) -> bool {
    if fdw_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    // setup tracing first
    tracing::subscriber::set_global_default(ipc_tracing::subscriber()).unwrap();

    // setup tokio runtime
    let Ok(rt) = Runtime::new() else {
        error!("cannot create tokio runtime");
        return false;
    };
    let _guard = rt.enter();

    let pid = unsafe { GetCurrentProcessId() };
    let module_handle = dll_module.0 as usize;
    // setup first ipc server
    let server = match create_ipc_server(create_ipc_addr(pid, module_handle as u32), true) {
        Ok(server) => server,
        Err(err) => {
            error!("cannot open ipc server. err: {err:?}");
            return false;
        }
    };
    let create_server =
        move || create_ipc_server(create_ipc_addr(pid, module_handle as u32), false);

    thread::spawn(move || rt.block_on(run_server(module_handle, server, create_server)));
    true
}

/// Create a new IPC server using the given address.
fn create_ipc_server(addr: impl AsRef<OsStr>, first: bool) -> anyhow::Result<NamedPipeServer> {
    Ok(unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .create_with_security_attributes_raw(
                addr,
                &mut SECURITY_ATTRIBUTES {
                    nLength: 1,
                    lpSecurityDescriptor: &mut create_everyone_security_desc()
                        .context("failed to create Everyone security desc")?
                        as *mut _ as _,
                    bInheritHandle: BOOL(0),
                } as *mut _ as _,
            )?
    })
}

/// Create Windows security descriptor allowing read/write permission to Everyone.
fn create_everyone_security_desc() -> anyhow::Result<SECURITY_DESCRIPTOR> {
    let mut everyone_sid = PSID::default();
    unsafe {
        AllocateAndInitializeSid(
            &SECURITY_WORLD_SID_AUTHORITY,
            1,
            SECURITY_WORLD_RID as _,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut everyone_sid,
        )?;
    }
    defer!(unsafe {
        FreeSid(everyone_sid);
    });

    let access = EXPLICIT_ACCESS_A {
        grfAccessPermissions: GENERIC_READ.0 | GENERIC_WRITE.0,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_A {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: PSTR(everyone_sid.0.cast()),
            ..Default::default()
        },
    };

    let mut pacl: *mut ACL = 0 as _;
    unsafe {
        SetEntriesInAclA(Some(&[access]), None, &mut pacl).ok()?;
    }

    let mut security_desc = SECURITY_DESCRIPTOR::default();
    unsafe {
        InitializeSecurityDescriptor(
            PSECURITY_DESCRIPTOR(&mut security_desc as *mut _ as _),
            SECURITY_DESCRIPTOR_REVISION,
        )?;

        SetSecurityDescriptorDacl(
            PSECURITY_DESCRIPTOR(&mut security_desc as *mut _ as _),
            true,
            Some(pacl),
            false,
        )?;
    }

    Ok(security_desc)
}
