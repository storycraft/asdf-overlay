mod acl;

use anyhow::Context;
use asdf_overlay_common::ipc::create_ipc_addr;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows::{Win32::Security::SECURITY_ATTRIBUTES, core::BOOL};

use acl::everyone_security_desc;

/// Open a new IPC server.
pub fn open<const FIRST: bool>(pid: u32, module_handle: u32) -> anyhow::Result<NamedPipeServer> {
    let addr = create_ipc_addr(pid, module_handle);
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: 1,
        lpSecurityDescriptor: &mut everyone_security_desc()
            .context("failed to create Everyone security desc")?
            as *mut _ as _,
        bInheritHandle: BOOL(0),
    };

    Ok(unsafe {
        ServerOptions::new()
            .first_pipe_instance(FIRST)
            .create_with_security_attributes_raw(addr, &mut attrs as *mut _ as _)?
    })
}
