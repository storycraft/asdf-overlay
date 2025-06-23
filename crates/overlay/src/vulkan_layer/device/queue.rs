use core::slice;

use ash::vk::{self, Handle};
use tracing::{debug, error, trace};
use windows::Win32::Foundation::HWND;

use crate::{
    app::OverlayIpc,
    backend::{Backends, WindowBackend, renderers::Renderer},
    vulkan_layer::device::{DISPATCH_TABLE, get_queue_device, swapchain::get_swapchain_data},
};

pub extern "system" fn present(queue: vk::Queue, info: *const vk::PresentInfoKHR) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let device = get_queue_device(queue).unwrap();
    if OverlayIpc::connected() {
        unsafe {
            let info = &*info;
            for &swapchain in slice::from_raw_parts(info.p_swapchains, info.swapchain_count as _) {
                let data = get_swapchain_data(swapchain);

                if let Err(err) = Backends::with_or_init_backend(HWND(data.hwnd as _), |backend| {
                    draw_overlay(data.device, queue, backend)
                }) {
                    error!("Backends::with_or_init_backend failed. err: {err:?}");
                }
            }
        }
    }

    unsafe { (DISPATCH_TABLE.get(&device.as_raw()).unwrap().queue_present)(queue, info) }
}

fn draw_overlay(device: vk::Device, queue: vk::Queue, backend: &mut WindowBackend) {
    match backend.renderer {
        Some(Renderer::Vulkan(ref mut renderer)) => {}
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
    }
}
