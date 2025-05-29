pub mod surface;
pub mod ty;

pub use asdf_overlay_common as common;
pub use dll_syringe::process;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
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
        System::SystemServices::{SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID},
    },
    core::{BOOL, PSTR},
};

use core::time::Duration;
use std::{env::current_exe, path::PathBuf};

use anyhow::{Context, bail};
use asdf_overlay_common::ipc::{
    create_ipc_path,
    server::{IpcServerConn, IpcServerEventStream},
};
use dll_syringe::{
    Syringe,
    process::{OwnedProcess, Process},
};
use tokio::{net::windows::named_pipe::ServerOptions, select, time::sleep};

fn default_dll_path() -> PathBuf {
    dll_on_exe("asdf_overlay.dll")
}

/// Create dll path relative to current executable
pub fn dll_on_exe(name: &str) -> PathBuf {
    if let Ok(mut current) = current_exe() {
        current.pop();
        current.push(name);
        current
    } else {
        PathBuf::from(name)
    }
}

/// Inject overlay and create ipc connection
///
/// Name must be unique or it will fail if there is a connection with same name
pub async fn inject(
    name: String,
    process: OwnedProcess,
    dll_path: Option<PathBuf>,
    timeout: Option<Duration>,
) -> anyhow::Result<(IpcServerConn, IpcServerEventStream)> {
    let pipe = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .write_owner(true)
            .write_dac(true)
            .create_with_security_attributes_raw(
                create_ipc_path(&name, process.pid()?.get()),
                &mut SECURITY_ATTRIBUTES {
                    nLength: 1,
                    lpSecurityDescriptor: &mut create_everyone_security_desc()? as *mut _ as _,
                    bInheritHandle: BOOL(0),
                } as *mut _ as _,
            )?
    };

    {
        let injector = Syringe::for_process(process);
        let module = injector.inject(dll_path.unwrap_or_else(default_dll_path))?;

        let start = unsafe {
            injector
                .get_payload_procedure::<fn(String) -> bool>(module, "asdf_overlay_connect")?
                .context("cannot find overlay start fn")?
        };
        if !start.call(&name)? {
            bail!("overlay initialization failed");
        }
    }

    let connect = IpcServerConn::connect(pipe);
    let timeout = sleep(timeout.unwrap_or(Duration::from_secs(10)));
    let conn = select! {
        res = connect => res?,
        _ = timeout => bail!("client wait timeout"),
    };

    Ok(conn)
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
