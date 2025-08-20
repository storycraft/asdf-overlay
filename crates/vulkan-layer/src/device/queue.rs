use core::{ptr, slice};

use asdf_overlay::{
    backend::{Backends, WindowBackend, render::Renderer},
    renderer::vulkan::VulkanRenderer,
};
use ash::vk::{self, Handle};
use tracing::{debug, error, trace};
use windows::Win32::{
    Foundation::LUID,
    Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory4},
};

use crate::{
    device::{
        DISPATCH_TABLE, DispatchTable, get_queue_data,
        swapchain::{SwapchainData, get_swapchain_data},
    },
    instance::physical_device::{get_physical_device_luid, get_physical_device_memory_properties},
};

pub(super) extern "system" fn present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let queue_data = get_queue_data(queue).unwrap();
    let mut table = DISPATCH_TABLE.get_mut(&queue_data.device.as_raw()).unwrap();
    let info = unsafe { &*info };
    let wait_semaphores =
        unsafe { slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as _) };
    let swapchains = unsafe { slice::from_raw_parts(info.p_swapchains, info.swapchain_count as _) };
    let indices = unsafe { slice::from_raw_parts(info.p_image_indices, info.swapchain_count as _) };

    for i in 0..info.swapchain_count as usize {
        let swapchain = swapchains[i];
        let index = indices[i];
        let data = get_swapchain_data(swapchain);

        let physical_device = table.physical_device;
        if let Err(err) = Backends::with_or_init_backend(
            data.hwnd,
            || {
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
            },
            |backend| {
                let semaphore = draw_overlay(
                    &table,
                    swapchain,
                    index,
                    &data,
                    queue,
                    queue_data.family_index,
                    backend,
                    wait_semaphores,
                );

                if let Some(semaphore) = semaphore {
                    table.semaphore_buf.push(semaphore);
                }
            },
        ) {
            error!("Backends::with_or_init_backend failed. err: {err:?}");
        }
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

    unsafe { (table.queue_present.unwrap())(queue, info) }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn draw_overlay(
    table: &DispatchTable,
    swapchain: vk::SwapchainKHR,
    index: u32,
    data: &SwapchainData,
    queue: vk::Queue,
    queue_family_index: u32,
    backend: &WindowBackend,
    wait_semaphores: &[vk::Semaphore],
) -> Option<vk::Semaphore> {
    let render = &mut *backend.render.lock();
    let renderer = match render.renderer {
        Some(Renderer::Vulkan(ref mut renderer)) => renderer,
        Some(_) => {
            trace!("ignoring vulkan rendering");
            return None;
        }
        None => {
            debug!("Found vulkan window");
            render.renderer = Some(Renderer::Vulkan(None));
            // wait next swap for possible dxgi swapchain check
            return None;
        }
    };

    let renderer = renderer.get_or_insert_with(|| {
        debug!("initializing vulkan renderer");

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
            .expect("failed to get swapchain images");
        };

        Box::new(
            VulkanRenderer::new(
                table.device.clone(),
                queue_family_index,
                data.image_size,
                data.format,
                &images,
            )
            .expect("renderer creation failed"),
        )
    });

    if render.surface.invalidate_update() {
        let props = get_physical_device_memory_properties(table.physical_device).unwrap();

        if let Err(err) = renderer.update_texture(
            render.surface.get().map(|surface| surface.texture()),
            data.format,
            &props,
        ) {
            error!("failed to update opengl texture. err: {err:?}");
            return None;
        }
    }
    let surface = render.surface.get()?;

    let res = renderer.draw(
        queue,
        wait_semaphores,
        index,
        render.position,
        surface.size(),
        render.window_size,
    );
    trace!("vulkan render: {:?}", res);
    res.ok().flatten()
}
