use core::ffi::c_void;

use anyhow::Context;
use once_cell::sync::Lazy;
use tracing::{debug, trace};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D12::{
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
            D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device,
        },
        Dxgi::IDXGISwapChain1,
    },
    core::Interface,
};

use crate::{
    backend::{Backends, render::Renderer},
    types::IntDashMap,
};

use super::HOOK;

pub type ExecuteCommandListsFn = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void);

struct WeakID3D12CommandQueue(*mut c_void);
unsafe impl Send for WeakID3D12CommandQueue {}
unsafe impl Sync for WeakID3D12CommandQueue {}

static QUEUE_MAP: Lazy<IntDashMap<usize, WeakID3D12CommandQueue>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn get_queue_for(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    Some(unsafe {
        ID3D12CommandQueue::from_raw_borrowed(&QUEUE_MAP.remove(&(device.as_raw() as _))?.1.0)
            .unwrap()
            .clone()
    })
}

#[tracing::instrument]
pub fn cleanup_swapchain(swapchain: &IDXGISwapChain1) {
    let hwnd = unsafe { swapchain.GetHwnd() }.ok();

    let Some(hwnd) = hwnd else {
        return;
    };

    // We don't know if they are trying clean up entire device, so cleanup everything
    _ = Backends::with_backend(hwnd, |backend| {
        let render = &mut *backend.render.lock();

        let Some(Renderer::Dx12(ref mut renderer)) = render.renderer else {
            return;
        };
        debug!("dx12 renderer cleanup");

        QUEUE_MAP.clear();
        renderer.take();
        render.cx.dx12.take();
        render.set_surface_updated();
    });
}

#[tracing::instrument]
pub extern "system" fn hooked_execute_command_lists(
    this: *mut c_void,
    num_command_lists: u32,
    pp_commmand_lists: *const *mut c_void,
) {
    trace!("ExecuteCommandLists called");

    unsafe {
        let queue = ID3D12CommandQueue::from_raw_borrowed(&this).unwrap();

        if queue.GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT {
            let mut device = None;
            queue.GetDevice::<ID3D12Device>(&mut device).unwrap();
            let device = device.unwrap();

            trace!(
                "found DIRECT command queue {:?} for device {:?}",
                queue, device
            );
            QUEUE_MAP.insert(device.as_raw() as _, WeakID3D12CommandQueue(queue.as_raw()));
        }

        HOOK.execute_command_lists.wait().original_fn()(this, num_command_lists, pp_commmand_lists)
    }
}

/// Get pointer to ID3D12CommandQueue::ExecuteCommandLists of D3D12_COMMAND_LIST_TYPE_DIRECT type by creating dummy device
#[tracing::instrument]
pub fn get_execute_command_lists_addr() -> anyhow::Result<ExecuteCommandListsFn> {
    unsafe {
        let mut device = None;
        D3D12CreateDevice::<_, ID3D12Device>(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
        let device = device.context("cannot create IDirect3DDevice12")?;

        let queue = device.CreateCommandQueue::<ID3D12CommandQueue>(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            ..Default::default()
        })?;
        let addr = Interface::vtable(&queue).ExecuteCommandLists;
        debug!("ID3D12CommandQueue::ExecuteCommandLists found: {:p}", addr);

        Ok(addr)
    }
}
