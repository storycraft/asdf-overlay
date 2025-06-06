mod dx;
mod opengl;
mod proc;

pub use dx::util::call_original_execute_command_lists;

use anyhow::Context;
use windows::Win32::Foundation::HWND;

#[tracing::instrument]
pub fn install(dummy_hwnd: HWND) -> anyhow::Result<()> {
    proc::hook().context("DispatchMessage hook initialization failed")?;
    dx::hook(dummy_hwnd).context("Direct3D hook initialization failed")?;
    opengl::hook(dummy_hwnd).context("OpenGL hook initialization failed")?;

    Ok(())
}
