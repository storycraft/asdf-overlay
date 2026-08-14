#![windows_subsystem = "windows"]

//! Official DLL crate for attaching [`asdf_overlay`] to other processes.
//! Using this DLL, the overlay can be controlled via cross-process IPC.
//!
//! Injection can be done using `asdf-overlay-client` crate.

mod cursors;
mod event_sink;
mod ipc;
mod ipc_tracing;
mod server;

extern crate asdf_overlay_vulkan_layer;

use anyhow::Context;
use asdf_overlay::initialize;
use asdf_overlay_common::event::OverlayEvent;
use asdf_overlay_window::Backends;
use core::time::Duration;
use std::{sync::Arc, thread};
use tokio::{net::windows::named_pipe::NamedPipeServer, runtime::Builder, time::sleep};
use tracing::{debug, error, warn};
use windows::Win32::{
    Foundation::HINSTANCE,
    System::{SystemServices::DLL_PROCESS_ATTACH, Threading::GetCurrentProcessId},
};

use crate::event_sink::EventSink;

async fn run(module_handle: usize, mut server: NamedPipeServer) -> anyhow::Result<()> {
    // initialize overlay
    initialize().context("overlay initialization")?;
    debug!("Overlay initialized.");

    // initialize windows
    let backends = Arc::new(Backends::new().context("window initialization")?);
    debug!("Window backend initialized.");

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

    loop {
        debug!("Waiting ipc client...");
        match server.connect().await {
            Ok(_) => {
                if let Err(err) = ipc::run(module_handle, backends.clone(), server).await {
                    warn!(error = ?err, "Client connection ended unexpectedly.");
                }
            }

            Err(err) => {
                error!(error = ?err, "Failed to connect to client.");
            }
        }

        server = next_ipc_server(module_handle as _).await;
    }
}

/// Initialize first IPC server.
///
/// NOTE: Loader lock is held when this function is called, so it must not block.
fn first_ipc_server(module_handle: usize) -> anyhow::Result<NamedPipeServer> {
    let pid = unsafe { GetCurrentProcessId() };

    // setup first ipc server
    server::open::<true>(pid, module_handle as _)
}

async fn next_ipc_server(module_handle: u32) -> NamedPipeServer {
    let pid = unsafe { GetCurrentProcessId() };

    loop {
        match server::open::<false>(pid, module_handle) {
            Ok(server) => return server,

            Err(err) => {
                error!(error = ?err, "Failed to create server. retrying after 5 seconds.");
                sleep(Duration::from_secs(5)).await;
            }
        }
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

    // setup tracing
    tracing::subscriber::set_global_default(ipc_tracing::subscriber()).unwrap();

    // Setup tokio runtime
    let rt = match Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(err) => {
            error!(error = ?err, "Cannot setup tokio runtime");
            return true;
        }
    };

    let module_handle = dll_module.0 as usize;
    let server = {
        let _guard = rt.enter();
        match first_ipc_server(module_handle) {
            Ok(server) => server,
            Err(err) => {
                error!(error = ?err, "Failed to create first ipc server.");
                return true;
            }
        }
    };

    thread::spawn(move || {
        if let Err(err) = rt.block_on(run(module_handle, server)) {
            error!(error = ?err, "Error occurred while running main");
        }
    });
    true
}
