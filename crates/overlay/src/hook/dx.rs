mod dx12;
mod dx9;
mod dxgi;
pub mod util;

use parking_lot::RwLock;
use tracing::debug;
use windows::Win32::Foundation::HWND;

use super::DetourHook;

#[derive(Default)]
struct Hook {
    present: Option<DetourHook>,
    present1: Option<DetourHook>,
    resize_buffers: Option<DetourHook>,
    execute_command_lists: Option<DetourHook>,
    end_scene: Option<DetourHook>,
}

static HOOK: RwLock<Hook> = RwLock::new(Hook {
    present: None,
    present1: None,
    resize_buffers: None,
    execute_command_lists: None,
    end_scene: None,
});

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let mut hook = HOOK.write();

    // dx12
    if let Ok(execute_command_lists) = dx12::get_execute_command_lists_addr() {
        hook.execute_command_lists = Some(unsafe {
            debug!("hooking ID3D12CommandQueue::ExecuteCommandLists");
            DetourHook::attach(
                execute_command_lists as _,
                dx12::hooked_execute_command_lists as _,
            )?
        });
    }

    // dxgi
    let (present, resize_buffers, present1) = dxgi::get_dxgi_addr(dummy_hwnd)?;
    debug!("hooking IDXGISwapChain::Present");
    let present_hook = unsafe { DetourHook::attach(present as _, dxgi::hooked_present as _)? };
    hook.present = Some(present_hook);

    debug!("hooking IDXGISwapChain::ResizeBuffers");
    let resize_buffers_hook =
        unsafe { DetourHook::attach(resize_buffers as _, dxgi::hooked_resize_buffers as _)? };
    hook.resize_buffers = Some(resize_buffers_hook);

    if let Some(present1) = present1 {
        debug!("hooking IDXGISwapChain1::Present1");
        let present1_hook =
            unsafe { DetourHook::attach(present1 as _, dxgi::hooked_present1 as _)? };
        hook.present1 = Some(present1_hook);
    }

    // dx9
    debug!("hooking IDirect3DDevice9::EndScene");
    let end_scene_hook = unsafe {
        DetourHook::attach(
            dx9::get_end_scene_addr(dummy_hwnd)? as _,
            dx9::hooked_end_scene as _,
        )?
    };
    hook.end_scene = Some(end_scene_hook);

    Ok(())
}

#[tracing::instrument]
pub fn cleanup() {
    *HOOK.write() = Hook::default();
    dx12::cleanup();
}
