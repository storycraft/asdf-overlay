//! Collection of hooks required to intercept window events and rendering.

mod dx;
mod opengl;

pub mod util {
    pub use super::dx::original_execute_command_lists;
}

pub use dx::submit_command_queue;

use tracing::Level;

use crate::util::with_dummy_hwnd;

#[tracing::instrument(level = Level::DEBUG)]
/// Install various hooks.
pub fn install() -> anyhow::Result<()> {
    with_dummy_hwnd(|dummy_hwnd| {
        dx::hook(dummy_hwnd);
        opengl::hook(dummy_hwnd);

        Ok(())
    })?
}
