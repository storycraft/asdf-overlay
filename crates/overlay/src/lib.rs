#![windows_subsystem = "windows"]

#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
mod wgl {
    include!(concat!(env!("OUT_DIR"), "/wgl_bindings.rs"));
}

mod app;
mod backend;
mod hook;
mod reader;
mod renderer;
mod resources;
mod texture;
mod types;
mod util;

#[cfg(debug_assertions)]
mod dbg;
mod interop;
mod layout;
mod surface;
mod vulkan_layer;

use anyhow::Context;
use app::app;
use asdf_overlay_common::ipc::create_ipc_addr;
use once_cell::sync::OnceCell;
use scopeguard::defer;
use std::{ffi::OsStr, thread};
use tokio::{
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    runtime::Runtime,
};
use tracing::{debug, error};
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

use crate::util::with_dummy_hwnd;

static INSTANCE: OnceCell<usize> = OnceCell::new();

pub fn instance() -> HINSTANCE {
    HINSTANCE(*INSTANCE.get().unwrap() as _)
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

    _ = INSTANCE.set(dll_module.0 as _);

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
    let Ok(server) = create_ipc_server(create_ipc_addr(pid, module_handle), true) else {
        error!("cannot open ipc server");
        return false;
    };
    let create_server = move || create_ipc_server(create_ipc_addr(pid, module_handle), false);

    thread::spawn(move || {
        // setup hook
        with_dummy_hwnd(|dummy_hwnd| {
            hook::install(dummy_hwnd).expect("hook initialization failed");
            debug!("hook installed");
        })
        .expect("failed to create dummy window");

        rt.block_on(app(server, create_server))
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
