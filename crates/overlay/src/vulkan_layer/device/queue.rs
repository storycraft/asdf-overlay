use core::slice;

use ash::vk::{self, Handle};
use tracing::{debug, error, trace};
use windows::Win32::Foundation::HWND;

use crate::{
    app::OverlayIpc,
    backend::{Backends, WindowBackend, renderers::Renderer},
    renderer::vulkan::VulkanRenderer,
    vulkan_layer::{
        device::{
            DISPATCH_TABLE, DispatchTable, get_queue_data,
            swapchain::{SwapchainData, get_swapchain_data},
        },
        instance::physical_device::get_physical_device_memory_properties,
    },
};

pub(super) extern "system" fn present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let queue_data = get_queue_data(queue).unwrap();
    let mut table = DISPATCH_TABLE.get_mut(&queue_data.device.as_raw()).unwrap();
    if OverlayIpc::connected() {
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
            let data = get_swapchain_data(swapchain);

            if let Err(err) = Backends::with_or_init_backend(HWND(data.hwnd as _), |backend| {
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
            }) {
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
    backend: &mut WindowBackend,
    wait_semaphores: &[vk::Semaphore],
) -> Option<vk::Semaphore> {
    let renderer = match backend.renderer {
        Some(Renderer::Vulkan(ref mut renderer)) => renderer,
        Some(_) => {
            trace!("ignoring vulkan rendering");
            return None;
        }
        None => {
            debug!("Found vulkan window");
            backend.renderer = Some(Renderer::Vulkan(None));
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
            VulkanRenderer::new(table.device.clone(), queue_family_index, data, &images)
                .expect("renderer creation failed"),
        )
    });

    if backend.surface.invalidate_update() {
        let props = get_physical_device_memory_properties(table.physical_device).unwrap();

        if let Err(err) = renderer.update_texture(
            backend.surface.get().map(|surface| surface.texture()),
            &props,
        ) {
            error!("failed to update opengl texture. err: {err:?}");
            return None;
        }
    }
    let surface = backend.surface.get()?;

    let screen = backend.size;
    let size = surface.size();
    let position = backend
        .layout
        .get_or_calc((size.0 as _, size.1 as _), screen);

    let res = renderer.draw(queue, wait_semaphores, index, position, size, screen);
    trace!("vulkan render: {:?}", res);
    res.ok().flatten()
}
