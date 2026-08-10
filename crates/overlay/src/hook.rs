//! Collection of hooks required to intercept window events and rendering.

mod dx;
mod opengl;

pub mod util {
    pub use super::dx::original_execute_command_lists;
}

use crate::util::with_dummy_hwnd;

#[tracing::instrument]
/// Install various hooks.
pub fn install() -> anyhow::Result<()> {
    with_dummy_hwnd(|dummy_hwnd| {
        dx::hook(dummy_hwnd);
        opengl::hook(dummy_hwnd);

        Ok(())
    })?
}
