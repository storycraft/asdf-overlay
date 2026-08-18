mod rtv;
mod util;

use asdf_overlay_event::{SurfaceInfo, SurfaceType};
use parking_lot::Once;
pub use util::original_execute_command_lists;

use core::ffi::c_void;

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use dashmap::Entry;
use once_cell::sync::{Lazy, OnceCell};
use tracing::{Level, debug, error, info, trace};
use windows::{
    Win32::Graphics::{
        Direct3D12::{
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
            D3D12_COMMAND_QUEUE_FLAG_NONE, ID3D12CommandQueue, ID3D12Device,
        },
        Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4, IDXGISwapChain1, IDXGISwapChain3},
    },
    core::Interface,
};

use crate::{
    hook::dx::{
        dx12::rtv::RtvDescriptors, dxgi::callback::register_swapchain_destruction_callback,
    },
    interop::DxInterop,
    renderer::dx12::Dx12Renderer,
    surface::{SurfaceState, Surfaces},
    types::IntDashMap,
};

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
            info!("initializing dx12 renderer");
            let device = unsafe { swapchain.GetDevice::<ID3D12Device>()? };

            let ref_mut = entry.insert(RendererData {
                renderer: Dx12Renderer::new(&device, swapchain)?,
                rtv: RtvDescriptors::new(&device)?,
            });
            register_swapchain_destruction_callback(swapchain, {
                let device = device.as_raw() as usize;
                move |this| cleanup_swapchain(this, device)
            });

            ref_mut
        }
    };

    f(&mut data)
}

#[tracing::instrument(level = Level::TRACE)]
fn get_queue_for(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    Some(unsafe {
        ID3D12CommandQueue::from_raw_borrowed(&QUEUE_MAP.remove(&(device.as_raw() as _))?.1.0)
            .unwrap()
            .clone()
    })
}

pub fn draw_overlay(state: &SurfaceState, device: &ID3D12Device, swapchain: &IDXGISwapChain3) {
    let Some(queue) = get_queue_for(device) else {
        debug!("Queue is not found for Direct3D12 device");
        return;
    };

    let Some(size) = state.texture_size() else {
        return;
    };

    let position = state.position();
    let screen = state.size();
    _ = with_or_init_renderer_data(swapchain, move |data| {
        trace!("using dx12 renderer");
        if state.texture.take_update()
            && let Err(err) = data
                .renderer
                .update_texture(device, state.texture.get().as_ref())
        {
            error!("failed to update dx12 texture: {err:?}");
        }

        let backbuffer_index = unsafe { swapchain.GetCurrentBackBufferIndex() };
        let res = data
            .rtv
            .with_next_swapchain(device, swapchain, backbuffer_index as _, |desc| {
                data.renderer.draw(
                    device,
                    swapchain,
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

pub(super) fn setup_fn(
    device: &ID3D12Device,
    swapchain: &IDXGISwapChain1,
) -> anyhow::Result<SurfaceState> {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Err(err) = hook(device) {
            error!("failed to hook dx12. err: {err:?}");
        }
    });

    let desc = unsafe { swapchain.GetDesc1() }?;
    let interop = DxInterop::new(get_dxgi_adapter(device).as_ref())?;
    let window_id = unsafe { swapchain.GetHwnd() }
        .ok()
        .map(|hwnd| hwnd.0 as u32);
    let gpu_id = interop.gpu_id;
    SurfaceState::new(
        interop,
        (desc.Width, desc.Height),
        SurfaceInfo {
            api: SurfaceType::Direct3D12 { window_id },
            gpu_id,
        },
    )
}

fn get_dxgi_adapter(device: &ID3D12Device) -> Option<IDXGIAdapter> {
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory4>() }.ok()?;
    let luid = unsafe { device.GetAdapterLuid() };
    unsafe { factory.EnumAdapterByLuid::<IDXGIAdapter>(luid) }.ok()
}

pub fn resize_swapchain(swapchain: &IDXGISwapChain1) {
    let Some(mut data) = RENDERERS.get_mut(&(swapchain.as_raw() as _)) else {
        return;
    };

    // invalidate old rtv descriptors
    data.rtv.reset();
}

#[tracing::instrument(level = Level::TRACE)]
fn cleanup_swapchain(swapchain: usize, device: usize) {
    if RENDERERS.remove(&swapchain).is_none() {
        return;
    };
    info!("dx12 renderer cleanup");

    QUEUE_MAP.remove(&device);
    Surfaces::cleanup_state(swapchain as _);
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_execute_command_lists(
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

type ExecuteCommandListsFn = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void);

struct Hook {
    execute_command_lists: OnceCell<DetourHook<ExecuteCommandListsFn>>,
}

static HOOK: Hook = Hook {
    execute_command_lists: OnceCell::new(),
};

pub fn hook(device: &ID3D12Device) -> anyhow::Result<()> {
    let execute_command_lists =
        get_execute_command_lists_addr(device).context("failed to load dx12 addrs")?;
    HOOK.execute_command_lists.get_or_try_init(|| unsafe {
        debug!("hooking ID3D12CommandQueue::ExecuteCommandLists");
        DetourHook::attach(execute_command_lists, hooked_execute_command_lists as _)
    })?;

    Ok(())
}

/// Get pointer to ID3D12CommandQueue::ExecuteCommandLists of D3D12_COMMAND_LIST_TYPE_DIRECT
#[tracing::instrument(level = Level::TRACE)]
fn get_execute_command_lists_addr(device: &ID3D12Device) -> anyhow::Result<ExecuteCommandListsFn> {
    unsafe {
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
