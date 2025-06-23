use ash::vk::{self, AllocationCallbacks, Handle};
use once_cell::sync::Lazy;
use tracing::{debug, trace};
use windows::Win32::Foundation::HWND;

use crate::{
    backend::{Backends, renderers::Renderer},
    types::IntDashMap,
    vulkan_layer::{device::DISPATCH_TABLE, instance::surface::get_surface_hwnd},
};

#[derive(Clone, Copy)]
pub struct SwapchainData {
    pub device: vk::Device,
    pub hwnd: u32,
    pub format: vk::Format,
}

static SWAPCHAIN_MAP: Lazy<IntDashMap<u64, SwapchainData>> = Lazy::new(IntDashMap::default);

pub fn get_swapchain_data(swapchain: vk::SwapchainKHR) -> SwapchainData {
    *SWAPCHAIN_MAP.get(&swapchain.as_raw()).unwrap()
}

pub extern "system" fn create_swapchain(
    device: vk::Device,
    create_info: *const vk::SwapchainCreateInfoKHR,
    callback: *const vk::AllocationCallbacks,
    swapchain: *mut vk::SwapchainKHR,
) -> vk::Result {
    trace!("vkCreateSwapchainKHR called");

    let info = unsafe { &*create_info };
    if !info.old_swapchain.is_null() {
        cleanup_swapchain(info.old_swapchain);
    }

    let res = unsafe {
        (DISPATCH_TABLE
            .get(&device.as_raw())
            .unwrap()
            .swapchain_fn
            .create_swapchain_khr)(device, create_info, callback, swapchain)
    };
    if res != vk::Result::SUCCESS {
        return res;
    }

    debug!("initializing swapchain data");
    let swapchain = unsafe { *swapchain }.as_raw();
    let hwnd = get_surface_hwnd(info.surface);
    SWAPCHAIN_MAP.insert(
        swapchain,
        SwapchainData {
            device,
            hwnd,
            format: info.image_format,
        },
    );

    vk::Result::SUCCESS
}

pub extern "system" fn destroy_swapchain(
    device: vk::Device,
    swapchain: vk::SwapchainKHR,
    callback: *const AllocationCallbacks,
) {
    trace!("vkDestroySwapchainKHR called");

    let table = DISPATCH_TABLE.get(&device.as_raw()).unwrap();
    cleanup_swapchain(swapchain);

    unsafe { (table.swapchain_fn.destroy_swapchain_khr)(device, swapchain, callback) }
}

fn cleanup_swapchain(swapchain: vk::SwapchainKHR) {
    let data = get_swapchain_data(swapchain);

    _ = Backends::with_backend(HWND(data.hwnd as _), |backend| {
        let Some(Renderer::Vulkan(ref mut renderer)) = backend.renderer else {
            return;
        };
        debug!("vulkan renderer cleanup");
        renderer.take();
    });
}
