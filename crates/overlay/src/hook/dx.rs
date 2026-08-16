mod dx11;
mod dx12;
mod dx9;
mod dxgi;

pub use dx12::{original_execute_command_lists, submit_command_queue};

use tracing::{Level, error};
use windows::Win32::Foundation::HWND;

#[tracing::instrument(level = Level::DEBUG)]
pub fn hook(dummy_hwnd: HWND) {
    if let Err(err) = dx12::hook() {
        error!("failed to hook dx12. err: {err:?}");
    }

    if let Err(err) = dxgi::hook(dummy_hwnd) {
        error!("failed to hook dxgi. err: {err:?}");
    }

    if let Err(err) = dx9::hook(dummy_hwnd) {
        error!("failed to hook dx9. err: {err:?}");
    }
}
