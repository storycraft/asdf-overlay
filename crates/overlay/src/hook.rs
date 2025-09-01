//! Collection of hooks required to intercept window events and rendering.

mod dx;
mod opengl;
mod proc;

pub mod util {
    pub use super::dx::original_execute_command_lists;
}

use anyhow::Context;
use windows::Win32::Foundation::HINSTANCE;

use crate::util::with_dummy_hwnd;

#[tracing::instrument]
/// Install various hooks.
pub fn install(hinstance: HINSTANCE) -> anyhow::Result<()> {
    with_dummy_hwnd(hinstance, |dummy_hwnd| {
        proc::hook().context("Proc hook failed")?;
        dx::hook(dummy_hwnd);
        opengl::hook(dummy_hwnd);

        Ok(())
    })?
}
