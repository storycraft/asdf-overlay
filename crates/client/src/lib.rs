//! Library for attaching `asdf-overlay` to a process and initiating IPC channel.
//!
//! By utilizing this library, you can render overlay from any process and control it via IPC.
//! It's designed to give you maximum flexibility as you can keep most of the logic in this process.
//!
//! # Example
//! ```no_run
//! use std::path::Path;
//! use std::time::Duration;
//! use asdf_overlay_client::{inject, OverlayDll};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let dll = OverlayDll {
//!         x64: Some(Path::new("asdf-overlay-x64.dll")),
//!         x86: Some(Path::new("asdf-overlay-x86.dll")),
//!         x86: Some(Path::new("asdf-overlay-arm64.dll")),
//!     };
//!
//!    let (mut conn, mut events) = inject(
//!         1234, // target process pid
//!         dll, // overlay dll paths
//!         Some(Duration::from_secs(10)), // timeout for injection and ipc connection
//!    ).await?;
//!
//!   // Use `conn` to send requests to overlay, and `events` to receive events from the overlay.
//!
//!   Ok(())
//! }
//!

pub mod client;
mod injector;
pub mod surface;
pub mod ty;

pub use asdf_overlay_common as common;
pub use asdf_overlay_event as event;

use core::time::Duration;
use std::path::Path;

use anyhow::{Context, bail};
use asdf_overlay_common::ipc::create_ipc_addr;
use tokio::{net::windows::named_pipe::ClientOptions, select, time::sleep};

use crate::client::{IpcClientConn, IpcClientEventStream};

/// Paths to overlay DLLs for different architectures.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayDll<'a> {
    /// Path to DLL to be used for x64 applications.
    pub x64: Option<&'a Path>,

    /// Path to DLL to be used for x86 applications.
    pub x86: Option<&'a Path>,

    /// Path to DLL to be used for ARM64 applications.
    pub arm64: Option<&'a Path>,
}

/// Inject overlay DLL into target process and create IPC connection.
/// * If you didn't supply DLL path for the target architecture, it will return an error.
/// * If injection or IPC connection fails, it will return an error.
/// * If timeout is `None`, it may wait indefinitely.
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
