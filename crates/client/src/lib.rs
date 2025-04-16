pub mod prelude;

use core::time::Duration;
use std::{env::current_exe, path::PathBuf};

use anyhow::{Context, bail};
use asdf_overlay_common::ipc::{create_ipc_path, server::IpcServerConn};
pub use dll_syringe::process;
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
pub async fn inject(
    name: String,
    process: OwnedProcess,
    dll_path: Option<PathBuf>,
    timeout: Option<Duration>,
) -> anyhow::Result<IpcServerConn> {
    let pipe = ServerOptions::new()
        .first_pipe_instance(true)
        .create(create_ipc_path(&name, process.pid()?.get()))?;

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
