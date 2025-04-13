pub mod prelude;

use core::time::Duration;
use std::{env::current_exe, path::PathBuf};

use anyhow::bail;
use asdf_overlay_common::ipc::server::{IpcServerConn, create_ipc_server};
pub use dll_syringe::process;
use dll_syringe::{
    Syringe,
    process::{OwnedProcess, Process},
};
use tokio::{select, time::sleep};

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
    process: OwnedProcess,
    dll_path: Option<PathBuf>,
    timeout: Option<Duration>,
) -> anyhow::Result<IpcServerConn> {
    let pid = process.pid()?;

    let server = create_ipc_server(pid.get())?;
    {
        let injector = Syringe::for_process(process);
        injector.inject(dll_path.unwrap_or_else(default_dll_path))?;
    }

    let connect = IpcServerConn::connect(server);
    let timeout = sleep(timeout.unwrap_or(Duration::MAX));
    select! {
        res = connect => res,
        _ = timeout => bail!("client wait timeout"),
    }
}
