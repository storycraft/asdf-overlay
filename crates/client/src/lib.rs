//! Attach overlays to another process and control them through IPC.
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
//!         arm64: Some(Path::new("asdf-overlay-arm64.dll")),
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
//! ```

pub mod client;
mod injector;

pub use asdf_overlay_common as common;

use core::time::Duration;
use std::path::Path;

use anyhow::{Context, bail};
use asdf_overlay_common::ipc::create_ipc_addr;
use tokio::{net::windows::named_pipe::ClientOptions, select, time::sleep};

use crate::client::{IpcClientConn, IpcClientEventStream};

/// Overlay DLL paths by target architecture.
///
/// Provide an absolute path accessible to the target process for each architecture
/// you intend to inject into.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayDll<'a> {
    /// Path to DLL to be used for x64 applications.
    pub x64: Option<&'a Path>,

    /// Path to DLL to be used for x86 applications.
    pub x86: Option<&'a Path>,

    /// Path to DLL to be used for ARM64 applications.
    pub arm64: Option<&'a Path>,
}

/// Load the matching overlay DLL into `pid` and connect to it.
///
/// Injection blocks the calling thread. The timeout applies separately to injection
/// and connection setup; it is not an overall deadline. [`None`] waits indefinitely.
///
/// Returns an error if the architecture pair is unsupported, the matching DLL path
/// is missing, or injection or connection fails. Connecting is attempted once.
/// Failure or cancellation does not unload an injected DLL.
pub async fn inject(
    pid: u32,
    dll: OverlayDll<'_>,
    timeout: Option<Duration>,
) -> anyhow::Result<(IpcClientConn, IpcClientEventStream)> {
    let module_handle =
        injector::inject(pid, dll, timeout).context("failed to inject overlay DLL")?;
    let ipc_addr = create_ipc_addr(pid, module_handle);

    let connect = IpcClientConn::new(ClientOptions::new().open(ipc_addr)?);
    let timeout = sleep(timeout.unwrap_or(Duration::MAX));
    let conn = select! {
        res = connect => res?,
        _ = timeout => bail!("ipc client wait timeout"),
    };

    Ok(conn)
}
