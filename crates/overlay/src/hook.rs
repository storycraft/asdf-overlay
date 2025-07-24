mod dx;
mod opengl;
mod proc;
pub mod util;

use anyhow::Context;
use windows::Win32::Foundation::HWND;

#[tracing::instrument]
pub fn install(dummy_hwnd: HWND) -> anyhow::Result<()> {
    proc::hook().context("Proc hook failed")?;
    dx::hook(dummy_hwnd);
    opengl::hook(dummy_hwnd);

    Ok(())
}
