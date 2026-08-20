use core::{ptr, slice};

use anyhow::Context;
use asdf_overlay::{
    event_sink::OverlayEventSink,
    interop::DxInterop,
    surface::{SurfaceState, Surfaces},
};
use asdf_overlay_event::{SurfaceInfo, SurfaceType};
use ash::vk::{self, Handle};
use tracing::{debug, error, trace};
use windows::Win32::{
    Foundation::LUID,
    Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4},
};

use crate::{
    device::{
        DISPATCH_TABLE, DispatchTable, get_queue_data,
        swapchain::{SwapchainData, with_swapchain_data},
    },
    instance::{
        physical_device::{get_physical_device_luid, get_physical_device_memory_properties},
        surface::get_surface_hwnd,
    },
    renderer::VulkanRenderer,
};

/// Layer `vkQueuePresentKHR` implementation
pub(super) extern "system" fn present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let queue_data = get_queue_data(queue).unwrap();
    let mut table = DISPATCH_TABLE.get_mut(&queue_data.device.as_raw()).unwrap();

    if OverlayEventSink::connected() {
        let info = unsafe { &*info };
        let wait_semaphores = unsafe {
            slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as _)
        };
        let swapchains =
            unsafe { slice::from_raw_parts(info.p_swapchains, info.swapchain_count as _) };
        let indices =
            unsafe { slice::from_raw_parts(info.p_image_indices, info.swapchain_count as _) };

        for i in 0..info.swapchain_count as usize {
            let swapchain = swapchains[i];
            let index = indices[i];
            _ = with_swapchain_data(swapchain, |data| {
                let physical_device = table.physical_device;
                if let Err(err) = Surfaces::with(
                    data.surface.as_raw(),
                    || setup_fn(physical_device, data),
                    |backend| {
                        let semaphore = draw_overlay(
                            &table,
                            swapchain,
                            index,
                            data,
                            queue,
                            queue_data.family_index,
                            backend,
                            wait_semaphores,
                        )?;

                        if let Some(semaphore) = semaphore {
                            table.semaphore_buf.push(semaphore);
                        }

                        Ok(())
                    },
                ) {
                    error!("Vulkan overlay error. err: {err:?}");
                }
            });
        }

        if !table.semaphore_buf.is_empty() {
            let present_info = vk::PresentInfoKHR::default()
                .swapchains(swapchains)
                .image_indices(indices)
                .wait_semaphores(&table.semaphore_buf);
            let res = unsafe { (table.queue_present.unwrap())(queue, &present_info) };
            table.semaphore_buf.clear();
            return res;
        }
    }

    unsafe { (table.queue_present.unwrap())(queue, info) }
}

fn setup_fn(
    physical_device: vk::PhysicalDevice,
    data: &SwapchainData,
) -> anyhow::Result<SurfaceState> {
    let window_id = get_surface_hwnd(data.surface).context("invalid surface handle")?;
    let interop = DxInterop::new(get_dxgi_adapter(physical_device).as_ref())?;
    let gpu_id = interop.gpu_id;

    SurfaceState::new(
        interop,
        data.image_size,
        SurfaceInfo {
            api: SurfaceType::Vulkan { window_id },
            gpu_id,
        },
    )
}

fn get_dxgi_adapter(physical_device: vk::PhysicalDevice) -> Option<IDXGIAdapter> {
    let mut luid = LUID::default();
    unsafe {
        ptr::copy_nonoverlapping::<[u8; 8]>(
            &get_physical_device_luid(physical_device)?,
            &mut luid as *mut _ as _,
            1,
        );
    }
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory4>() }.ok()?;

    unsafe { factory.EnumAdapterByLuid(luid).ok() }
}

/// Draw the overlay, create a semaphore chained to the provided wait semaphores, and return it.
#[allow(clippy::too_many_arguments)]
#[inline]
fn draw_overlay(
    table: &DispatchTable,
    swapchain: vk::SwapchainKHR,
    index: u32,
    data: &SwapchainData,
    queue: vk::Queue,
    queue_family_index: u32,
    state: &SurfaceState,
    wait_semaphores: &[vk::Semaphore],
) -> anyhow::Result<Option<vk::Semaphore>> {
    let mut renderer = data.renderer.lock();
    let renderer = match *renderer {
        Some(ref mut renderer) => renderer,
        None => {
            debug!("Initializing Vulkan renderer");

            let mut image_count = 0;
            let mut images = Vec::<vk::Image>::new();
            unsafe {
                _ = (table.swapchain_fn.get_swapchain_images_khr)(
                    table.device.handle(),
                    swapchain,
                    &mut image_count,
                    0 as _,
                );
                images.resize(image_count as _, vk::Image::null());

                (table.swapchain_fn.get_swapchain_images_khr)(
                    table.device.handle(),
                    swapchain,
                    &mut image_count,
                    images.as_mut_ptr(),
                )
                .result()
                .context("getting swapchain images")?;
            };

            renderer.insert(
                VulkanRenderer::new(
                    table.device.clone(),
                    queue_family_index,
                    data.image_size,
                    data.format,
                    &images,
                )
                .context("renderer creation failed")?,
            )
        }
    };

    let Some(size) = state.texture_size() else {
        return Ok(None);
    };

    if state.texture.take_update() {
        let props = get_physical_device_memory_properties(table.physical_device).unwrap();

        renderer
            .update_texture(state.texture.get().as_ref(), data.format, &props)
            .context("updating renderer texture")?;
    }

    let position = state.position();
    let screen = state.size();
    renderer
        .draw(queue, wait_semaphores, index, position, size, screen)
        .context("renderer draw")
}
