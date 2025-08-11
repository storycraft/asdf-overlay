#![windows_subsystem = "windows"]

#[cfg(debug_assertions)]
mod dbg;

mod server;

extern crate asdf_overlay_vulkan_layer;

use anyhow::Context;
use asdf_overlay::{
    backend::{Backends, window::ListenInputFlags},
    event_sink::OverlayEventSink,
    initialize,
};
use asdf_overlay_common::{
    ipc::create_ipc_addr,
    request::{Request, WindowRequest},
};
use asdf_overlay_event::{ClientEvent, WindowEvent};
use core::time::Duration;
use scopeguard::defer;
use std::{ffi::OsStr, thread};
use tokio::{
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    runtime::Runtime,
    time::sleep,
};
use tracing::{debug, error, trace, warn};
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, HINSTANCE},
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

use crate::server::IpcServerConn;

#[tracing::instrument(skip(server))]
async fn run(server: NamedPipeServer) -> anyhow::Result<()> {
    fn handle_window_event(hwnd: u32, req: WindowRequest) -> anyhow::Result<bool> {
        let res = Backends::with_backend(hwnd, |backend| match req {
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
                backend.block_input(cmd.block);
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

    let mut conn = IpcServerConn::new(server).await?;
    let emitter = conn.create_emitter();
    {
        debug!("sending initial data");
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
    }

    OverlayEventSink::set(move |event| _ = emitter.emit(event));
    defer!({
        debug!("cleanup start");
        OverlayEventSink::clear();
        Backends::cleanup_backends();
    });

    while let Ok((req_id, req)) = conn.recv().await {
        trace!("recv id: {req_id} req: {req:?}");

        match req {
            Request::Window { id, request } => {
                conn.reply(req_id, handle_window_event(id, request)?)?;
            }
        }
    }
    conn.close().await?;
    Ok(())
}

#[tracing::instrument(skip(create_server))]
pub async fn run_server(
    mut server: NamedPipeServer,
    mut create_server: impl FnMut() -> anyhow::Result<NamedPipeServer>,
) {
    loop {
        debug!("waiting ipc client...");
        match server.connect().await {
            Ok(_) => {
                if let Err(err) = run(server).await {
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

#[unsafe(no_mangle)]
#[allow(non_snake_case, unused_variables)]
/// # Safety
/// Can be called by loader only. Must not be called manually.
pub unsafe extern "system" fn DllMain(dll_module: HINSTANCE, fdw_reason: u32, _: *mut ()) -> bool {
    #[cfg(debug_assertions)]
    fn setup_tracing() {
        use tracing::level_filters::LevelFilter;

        use crate::dbg::WinDbgMakeWriter;

        tracing_subscriber::fmt::fmt()
            .with_ansi(false)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::TRACE)
            .with_writer(WinDbgMakeWriter::new())
            .init();
    }

    if fdw_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    // setup tracing first
    #[cfg(debug_assertions)]
    setup_tracing();

    // setup tokio runtime
    let Ok(rt) = Runtime::new() else {
        error!("cannot create tokio runtime");
        return false;
    };
    let _guard = rt.enter();

    let pid = unsafe { GetCurrentProcessId() };
    let module_handle = dll_module.0 as u32;
    // setup first ipc server
    let server = match create_ipc_server(create_ipc_addr(pid, module_handle), true) {
        Ok(server) => server,
        Err(err) => {
            error!("cannot open ipc server. err: {err:?}");
            return false;
        }
    };
    let create_server = move || create_ipc_server(create_ipc_addr(pid, module_handle), false);

    thread::spawn(move || {
        // initialize overlay
        initialize(module_handle as _).expect("initialization failed");
        debug!("hook installed");

        rt.block_on(run_server(server, create_server))
    });
    true
}

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
