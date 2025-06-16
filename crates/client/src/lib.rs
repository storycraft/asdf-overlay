pub mod injector;
pub mod surface;
pub mod ty;

pub use asdf_overlay_common as common;

use core::time::Duration;
use std::path::Path;

use anyhow::{Context, bail};
use asdf_overlay_common::ipc::{
    client::{IpcClientConn, IpcClientEventStream},
    create_ipc_addr,
};
use tokio::{net::windows::named_pipe::ClientOptions, select, time::sleep};

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayDll<'a> {
    pub x64: Option<&'a Path>,
    pub x86: Option<&'a Path>,
    pub arm64: Option<&'a Path>,
}

/// Inject overlay and create ipc connection
pub async fn inject(
    pid: u32,
    dll: OverlayDll<'_>,
    timeout: Option<Duration>,
) -> anyhow::Result<(IpcClientConn, IpcClientEventStream)> {
    let module_handle =
        injector::inject(pid, dll, timeout).context("failed to inject overlay DLL")?;
    let ipc_addr = create_ipc_addr(pid, module_handle);

    let connect = IpcClientConn::new(ClientOptions::new().open(ipc_addr)?);
    let timeout = sleep(timeout.unwrap_or(Duration::from_secs(10)));
    let conn = select! {
        res = connect => res?,
        _ = timeout => bail!("ipc client wait timeout"),
    };

    Ok(conn)
}
