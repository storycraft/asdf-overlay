mod dx12;
mod dx9;
mod dxgi;
pub mod util;

use once_cell::sync::OnceCell;
use tracing::debug;
use windows::Win32::Foundation::HWND;

use super::DetourHook;

#[derive(Default)]
struct Hook {
    present: OnceCell<DetourHook>,
    present1: OnceCell<DetourHook>,
    resize_buffers: OnceCell<DetourHook>,
    execute_command_lists: OnceCell<DetourHook>,
    end_scene: OnceCell<DetourHook>,
    reset: OnceCell<DetourHook>,
}

static HOOK: Hook = Hook {
    present: OnceCell::new(),
    present1: OnceCell::new(),
    resize_buffers: OnceCell::new(),
    execute_command_lists: OnceCell::new(),
    end_scene: OnceCell::new(),
    reset: OnceCell::new(),
};

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    // dx12
    if let Ok(execute_command_lists) = dx12::get_execute_command_lists_addr() {
        HOOK.execute_command_lists.get_or_try_init(|| unsafe {
            debug!("hooking ID3D12CommandQueue::ExecuteCommandLists");
            DetourHook::attach(
                execute_command_lists as _,
                dx12::hooked_execute_command_lists as _,
            )
        })?;
    }

    // dxgi
    let (present, resize_buffers, present1) = dxgi::get_dxgi_addr(dummy_hwnd)?;
    debug!("hooking IDXGISwapChain::Present");
    HOOK.present.get_or_try_init(|| unsafe {
        DetourHook::attach(present as _, dxgi::hooked_present as _)
    })?;

    debug!("hooking IDXGISwapChain::ResizeBuffers");
    HOOK.resize_buffers.get_or_try_init(|| unsafe {
        DetourHook::attach(resize_buffers as _, dxgi::hooked_resize_buffers as _)
    })?;

    if let Some(present1) = present1 {
        debug!("hooking IDXGISwapChain1::Present1");
        HOOK.present1.get_or_try_init(|| unsafe {
            DetourHook::attach(present1 as _, dxgi::hooked_present1 as _)
        })?;
    }

    // dx9
    let (end_scene, reset) = dx9::get_dx9_addr(dummy_hwnd)?;
    debug!("hooking IDirect3DDevice9::EndScene");
    HOOK.end_scene.get_or_try_init(|| unsafe {
        DetourHook::attach(end_scene as _, dx9::hooked_end_scene as _)
    })?;
    debug!("hooking IDirect3DDevice9::Reset");
    HOOK.reset
        .get_or_try_init(|| unsafe { DetourHook::attach(reset as _, dx9::hooked_reset as _) })?;

    Ok(())
}

#[tracing::instrument]
pub fn cleanup() {
    dx9::cleanup();
}
