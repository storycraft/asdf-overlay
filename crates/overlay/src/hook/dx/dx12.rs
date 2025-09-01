mod rtv;

use core::ffi::c_void;

use anyhow::Context;
use dashmap::Entry;
use once_cell::sync::Lazy;
use tracing::{debug, trace};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D12::{
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
            D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device,
        },
        Dxgi::{IDXGISwapChain1, IDXGISwapChain3},
    },
    core::Interface,
};

use crate::{
    backend::{WindowBackend, render::Renderer},
    hook::dx::{
        dx12::rtv::RtvDescriptors, dxgi::callback::register_swapchain_destruction_callback,
    },
    renderer::dx12::Dx12Renderer,
    types::IntDashMap,
};

use super::HOOK;

pub type ExecuteCommandListsFn = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void);

struct WeakID3D12CommandQueue(*mut c_void);
unsafe impl Send for WeakID3D12CommandQueue {}
unsafe impl Sync for WeakID3D12CommandQueue {}

static QUEUE_MAP: Lazy<IntDashMap<usize, WeakID3D12CommandQueue>> = Lazy::new(IntDashMap::default);

/// Mapping from [`IDXGISwapChain3`] to [`RendererData`].
static RENDERERS: Lazy<IntDashMap<usize, RendererData>> = Lazy::new(IntDashMap::default);

struct RendererData {
    renderer: Dx12Renderer,
    rtv: RtvDescriptors,
}

#[inline]
fn with_or_init_renderer_data<R>(
    swapchain: &IDXGISwapChain3,
    f: impl FnOnce(&mut RendererData) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let mut data = match RENDERERS.entry(swapchain.as_raw() as _) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            debug!("initializing dx12 renderer");
            let device = unsafe { swapchain.GetDevice::<ID3D12Device>()? };

            let ref_mut = entry.insert(RendererData {
                renderer: Dx12Renderer::new(&device, swapchain)?,
                rtv: RtvDescriptors::new(&device)?,
            });
            register_swapchain_destruction_callback(&swapchain, {
                let device = device.as_raw() as usize;
                move |this| cleanup_swapchain(this, device)
            });

            ref_mut
        }
    };

    f(&mut data)
}

#[tracing::instrument]
fn get_queue_for(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    Some(unsafe {
        ID3D12CommandQueue::from_raw_borrowed(&QUEUE_MAP.remove(&(device.as_raw() as _))?.1.0)
            .unwrap()
            .clone()
    })
}

pub fn draw_overlay(backend: &WindowBackend, device: &ID3D12Device, swapchain: &IDXGISwapChain3) {
    let Some(queue) = get_queue_for(device) else {
        return;
    };

    let mut render = backend.render.lock();
    match render.renderer {
        Some(Renderer::Dx12) => {}

        // drawing on opengl with dxgi swapchain can cause deadlock
        Some(Renderer::Opengl) => {
            render.renderer = Some(Renderer::Dx12);
            debug!("switching from opengl to dx12 render");
            // skip drawing on render changes
            return;
        }
        // use dxgi swapchain instead
        Some(Renderer::Vulkan) => {
            render.renderer = Some(Renderer::Dx12);
            debug!("switching from vulkan to dx12 render");
            return;
        }
        Some(_) => {
            trace!("ignoring dx12 rendering");
            return;
        }
        None => {
            render.renderer = Some(Renderer::Dx12);
            // skip drawing on renderer check
            return;
        }
    };

    let Some(surface) = render.surface.get() else {
        return;
    };

    let size = surface.size();
    let update = render.surface.take_update();
    let position = render.position;
    let screen = render.window_size;
    drop(render);

    _ = with_or_init_renderer_data(swapchain, move |data| {
        trace!("using dx12 renderer");
        if let Some(update) = update {
            data.renderer.update_texture(update);
        }

        let backbuffer_index = unsafe { swapchain.GetCurrentBackBufferIndex() };
        let res = data
            .rtv
            .with_next_swapchain(device, &swapchain, backbuffer_index as _, |desc| {
                data.renderer.draw(
                    device,
                    &swapchain,
                    backbuffer_index,
                    desc,
                    &queue,
                    position,
                    size,
                    screen,
                )
            });

        trace!("dx12 render: {:?}", res);
        res
    });
}

pub fn resize_swapchain(swapchain: &IDXGISwapChain1) {
    let Some(mut data) = RENDERERS.get_mut(&(swapchain.as_raw() as _)) else {
        return;
    };

    // invalidate old rtv descriptors
    data.rtv.reset();
}

#[tracing::instrument]
fn cleanup_swapchain(swapchain: usize, device: usize) {
    if RENDERERS.remove(&swapchain).is_none() {
        return;
    };
    debug!("dx12 renderer cleanup");

    QUEUE_MAP.remove(&device);
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
