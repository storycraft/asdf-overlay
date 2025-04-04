pub mod prelude;

use std::{env::current_exe, path::PathBuf};

use asdf_overlay_common::ipc::client::IpcClientConn;
pub use dll_syringe::process;
use dll_syringe::{
    Syringe,
    process::{OwnedProcess, Process},
};

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
) -> anyhow::Result<IpcClientConn> {
    let pid = process.pid()?;
    {
        let injector = Syringe::for_process(process);
        injector.inject(dll_path.unwrap_or_else(|| default_dll_path()))?;
    }

    IpcClientConn::connect(pid.get()).await
}
