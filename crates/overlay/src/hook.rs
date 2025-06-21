mod dx;
mod opengl;
mod proc;
mod vulkan;

pub use dx::util::call_original_execute_command_lists;

use anyhow::Context;
use windows::Win32::Foundation::HWND;

#[tracing::instrument]
pub fn install(dummy_hwnd: HWND) -> anyhow::Result<()> {
    proc::hook().context("Proc hook failed")?;
    dx::hook(dummy_hwnd);
    opengl::hook(dummy_hwnd);

    Ok(())
}
