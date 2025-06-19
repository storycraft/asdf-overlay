mod dx11;
mod dx12;
mod dx9;
mod dxgi;
pub mod util;

use asdf_overlay_hook::DetourHook;
use dx9::{EndSceneFn, ResetFn};
use dx12::ExecuteCommandListsFn;
use dxgi::{Present1Fn, PresentFn};
use once_cell::sync::OnceCell;
use tracing::debug;
use windows::Win32::Foundation::HWND;

use crate::hook::dx::{
    dx9::ResetExFn,
    dxgi::{ResizeBuffers1Fn, ResizeBuffersFn},
};

#[derive(Default)]
struct Hook {
    present: OnceCell<DetourHook<PresentFn>>,
    present1: OnceCell<DetourHook<Present1Fn>>,
    resize_buffers: OnceCell<DetourHook<ResizeBuffersFn>>,
    resize_buffers1: OnceCell<DetourHook<ResizeBuffers1Fn>>,
    execute_command_lists: OnceCell<DetourHook<ExecuteCommandListsFn>>,
    end_scene: OnceCell<DetourHook<EndSceneFn>>,
    reset: OnceCell<DetourHook<ResetFn>>,
    reset_ex: OnceCell<DetourHook<ResetExFn>>,
}

static HOOK: Hook = Hook {
    present: OnceCell::new(),
    present1: OnceCell::new(),
    resize_buffers: OnceCell::new(),
    resize_buffers1: OnceCell::new(),
    execute_command_lists: OnceCell::new(),
    end_scene: OnceCell::new(),
    reset: OnceCell::new(),
    reset_ex: OnceCell::new(),
};

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    // dx12
    if let Ok(execute_command_lists) = dx12::get_execute_command_lists_addr() {
        HOOK.execute_command_lists.get_or_try_init(|| unsafe {
            debug!("hooking ID3D12CommandQueue::ExecuteCommandLists");
            DetourHook::attach(
                execute_command_lists,
                dx12::hooked_execute_command_lists as _,
            )
        })?;
    }

    // dxgi
    let dxgi_functions = dxgi::get_dxgi_addr(dummy_hwnd)?;
    debug!("hooking IDXGISwapChain::Present");
    HOOK.present.get_or_try_init(|| unsafe {
        DetourHook::attach(dxgi_functions.present, dxgi::hooked_present as _)
    })?;

    if let Some(present1) = dxgi_functions.present1 {
        debug!("hooking IDXGISwapChain1::Present1");
        HOOK.present1.get_or_try_init(|| unsafe {
            DetourHook::attach(present1, dxgi::hooked_present1 as _)
        })?;
    }

    debug!("hooking IDXGISwapChain::ResizeBuffers");
    HOOK.resize_buffers.get_or_try_init(|| unsafe {
        DetourHook::attach(
            dxgi_functions.resize_buffers,
            dxgi::hooked_resize_buffers as _,
        )
    })?;

    if let Some(resize_buffers1) = dxgi_functions.resize_buffers1 {
        debug!("hooking IDXGISwapChain3::ResizeBuffers1");
        HOOK.resize_buffers1.get_or_try_init(|| unsafe {
            DetourHook::attach(resize_buffers1, dxgi::hooked_resize_buffers1 as _)
        })?;
    }

    // dx9
    let (end_scene, reset, reset_ex) = dx9::get_dx9_addr(dummy_hwnd)?;
    debug!("hooking IDirect3DDevice9::EndScene");
    HOOK.end_scene
        .get_or_try_init(|| unsafe { DetourHook::attach(end_scene, dx9::hooked_end_scene as _) })?;
    debug!("hooking IDirect3DDevice9::Reset");
    HOOK.reset
        .get_or_try_init(|| unsafe { DetourHook::attach(reset, dx9::hooked_reset as _) })?;
    debug!("hooking IDirect3DDevice9Ex::ResetEx");
    HOOK.reset_ex
        .get_or_try_init(|| unsafe { DetourHook::attach(reset_ex, dx9::hooked_reset_ex as _) })?;

    Ok(())
}
