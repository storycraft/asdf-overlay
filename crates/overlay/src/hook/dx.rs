mod dx11;
mod dx12;
mod dx9;
mod dxgi;
pub mod util;

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use dx9::{PresentExFn, PresentFn as Dx9PresentFn, ResetFn, SwapchainPresentFn};
use dx12::ExecuteCommandListsFn;
use dxgi::{Present1Fn, PresentFn};
use once_cell::sync::OnceCell;
use tracing::{debug, error};
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
    dx9_present: OnceCell<DetourHook<Dx9PresentFn>>,
    dx9_present_ex: OnceCell<DetourHook<PresentExFn>>,
    dx9_swapchain_present: OnceCell<DetourHook<SwapchainPresentFn>>,
    reset: OnceCell<DetourHook<ResetFn>>,
    reset_ex: OnceCell<DetourHook<ResetExFn>>,
}

static HOOK: Hook = Hook {
    present: OnceCell::new(),
    present1: OnceCell::new(),
    resize_buffers: OnceCell::new(),
    resize_buffers1: OnceCell::new(),
    execute_command_lists: OnceCell::new(),
    dx9_present: OnceCell::new(),
    dx9_present_ex: OnceCell::new(),
    dx9_swapchain_present: OnceCell::new(),
    reset: OnceCell::new(),
    reset_ex: OnceCell::new(),
};

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) {
    fn hook_dx12() -> anyhow::Result<()> {
        let execute_command_lists =
            dx12::get_execute_command_lists_addr().context("failed to load dx12 addrs")?;
        HOOK.execute_command_lists.get_or_try_init(|| unsafe {
            debug!("hooking ID3D12CommandQueue::ExecuteCommandLists");
            DetourHook::attach(
                execute_command_lists,
                dx12::hooked_execute_command_lists as _,
            )
        })?;

        Ok(())
    }

    fn hook_dxgi(dummy_hwnd: HWND) -> anyhow::Result<()> {
        let dxgi_functions =
            dxgi::get_dxgi_addr(dummy_hwnd).context("failed to load dxgi addrs")?;

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
        Ok(())
    }

    fn hook_dx9(dummy_hwnd: HWND) -> anyhow::Result<()> {
        let (present, swapchain_present, present_ex, reset, reset_ex) =
            dx9::get_dx9_addr(dummy_hwnd).context("failed to load dx9 addrs")?;

        debug!("hooking IDirect3DDevice9::Reset");
        HOOK.reset
            .get_or_try_init(|| unsafe { DetourHook::attach(reset, dx9::hooked_reset as _) })?;
        debug!("hooking IDirect3DDevice9Ex::ResetEx");
        HOOK.reset_ex.get_or_try_init(|| unsafe {
            DetourHook::attach(reset_ex, dx9::hooked_reset_ex as _)
        })?;
        debug!("hooking IDirect3DDevice9::Present");
        HOOK.dx9_present
            .get_or_try_init(|| unsafe { DetourHook::attach(present, dx9::hooked_present as _) })?;
        debug!("hooking IDirect3DSwapChain9::Present");
        HOOK.dx9_swapchain_present.get_or_try_init(|| unsafe {
            DetourHook::attach(swapchain_present, dx9::hooked_swapchain_present as _)
        })?;
        debug!("hooking IDirect3DDevice9Ex::PresentEx");
        HOOK.dx9_present_ex.get_or_try_init(|| unsafe {
            DetourHook::attach(present_ex, dx9::hooked_present_ex as _)
        })?;

        Ok(())
    }

    if let Err(err) = hook_dx12() {
        error!("failed to hook dx12. err: {err:?}");
    }

    if let Err(err) = hook_dxgi(dummy_hwnd) {
        error!("failed to hook dxgi. err: {err:?}");
    }

    if let Err(err) = hook_dx9(dummy_hwnd) {
        error!("failed to hook dxgi. err: {err:?}");
    }
}
