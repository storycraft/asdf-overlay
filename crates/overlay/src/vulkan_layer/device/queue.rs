use core::slice;

use ash::vk::{self, Handle};
use tracing::{debug, error, trace};
use windows::Win32::Foundation::HWND;

use crate::{
    app::OverlayIpc,
    backend::{Backends, WindowBackend, renderers::Renderer},
    renderer::vulkan::VulkanRenderer,
    vulkan_layer::device::{
        DISPATCH_TABLE, DispatchTable, get_queue_data,
        swapchain::{SwapchainData, get_swapchain_data},
    },
};

pub(super) extern "system" fn present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let queue_data = get_queue_data(queue).unwrap();
    let table = DISPATCH_TABLE.get(&queue_data.device.as_raw()).unwrap();
    if OverlayIpc::connected() {
        unsafe {
            let info = &*info;
            let swapchains = slice::from_raw_parts(info.p_swapchains, info.swapchain_count as _);
            let indices = slice::from_raw_parts(info.p_image_indices, info.swapchain_count as _);
            for i in 0..info.swapchain_count as usize {
                let swapchain = swapchains[i];
                let index = indices[i];
                let data = get_swapchain_data(swapchain);

                if let Err(err) = Backends::with_or_init_backend(HWND(data.hwnd as _), |backend| {
                    draw_overlay(
                        &table,
                        swapchain,
                        &data,
                        queue,
                        queue_data.family_index,
                        index,
                        backend,
                    )
                }) {
                    error!("Backends::with_or_init_backend failed. err: {err:?}");
                }
            }
        }
    }

    unsafe { (table.queue_present.unwrap())(queue, info) }
}

#[inline]
fn draw_overlay(
    table: &DispatchTable,
    swapchain: vk::SwapchainKHR,
    data: &SwapchainData,
    queue: vk::Queue,
    queue_family_index: u32,
    index: u32,
    backend: &mut WindowBackend,
) {
    let renderer = match backend.renderer {
        Some(Renderer::Vulkan(ref mut renderer)) => renderer,
        Some(_) => {
            trace!("ignoring vulkan rendering");
            return;
        }
        None => {
            debug!("Found vulkan window");
            backend.renderer = Some(Renderer::Vulkan(None));
            // wait next swap for possible dxgi swapchain check
            return;
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
        if let Err(err) =
            renderer.update_texture(backend.surface.get().map(|surface| surface.texture()))
        {
            error!("failed to update opengl texture. err: {err:?}");
            return;
        }
    }
    let Some(surface) = backend.surface.get() else {
        return;
    };

    let screen = backend.size;
    let size = surface.size();
    let position = backend
        .layout
        .get_or_calc((size.0 as _, size.1 as _), screen);

    let _res = renderer.draw(queue, index, position, size, screen);
    trace!("vulkan render: {:?}", _res);
}
