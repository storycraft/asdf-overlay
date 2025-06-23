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

    let res = unsafe {
        (DISPATCH_TABLE
            .get(&device.as_raw())
            .unwrap()
            .create_swapchain)(device, create_info, callback, swapchain)
    };
    if res != vk::Result::SUCCESS {
        return res;
    }

    debug!("initializing swapchain data");
    let create_info = unsafe { &*create_info };
    let swapchain = unsafe { *swapchain }.as_raw();
    let hwnd = get_surface_hwnd(create_info.surface);
    SWAPCHAIN_MAP.insert(swapchain, SwapchainData { device, hwnd });

    vk::Result::SUCCESS
}

pub extern "system" fn destroy_swapchain(
    device: vk::Device,
    swapchain: vk::SwapchainKHR,
    callback: *const AllocationCallbacks,
) {
    trace!("vkDestroySwapchainKHR called");

    let table = DISPATCH_TABLE.get(&device.as_raw()).unwrap();
    let data = get_swapchain_data(swapchain);

    _ = Backends::with_backend(HWND(data.hwnd as _), |backend| {
        let Some(Renderer::Vulkan(ref mut renderer)) = backend.renderer else {
            return;
        };
        debug!("vulkan renderer cleanup");
        renderer.take();
    });

    unsafe { (table.destroy_swapchain)(device, swapchain, callback) }
}
