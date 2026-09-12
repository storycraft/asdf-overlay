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

/// Load the matching overlay DLL into `pid` and open its IPC connection.
///
/// Returns a request connection and a separate event stream. Keep the connection
/// alive while receiving events; dropping it stops the background reader.
///
/// Supply a DLL matching the target architecture. Use an absolute path accessible
/// to the target: relative paths are resolved by the target's loader, and a file
/// that exists locally may still fail to load there. Injection from x86 into x64
/// is unsupported, as are other architecture pairs rejected by the injector.
///
/// # Caveats
/// Injection runs synchronously before the first await and can block the executor
/// thread. The timeout is applied separately to the remote-thread wait and client
/// construction, not as an overall deadline. The native wait truncates to whole
/// milliseconds and casts to `u32`; avoid durations at or above `u32::MAX`
/// milliseconds, which wrap or become an infinite wait. `None` can wait indefinitely.
/// Opening the pipe is attempted once, without retry or a protocol handshake.
/// Failure or cancellation does not unload an already injected DLL.
///
/// # Errors
/// Returns errors for missing architecture paths, unsupported architecture pairs,
/// process access, DLL loading, native wait timeout, or opening the named pipe.
/// Successful construction does not establish that the server can handle requests.
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
