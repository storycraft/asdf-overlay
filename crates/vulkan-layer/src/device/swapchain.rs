use asdf_overlay::backend::Backends;
use ash::vk::{self, AllocationCallbacks, Handle};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tracing::{debug, trace};

use crate::{
    device::DISPATCH_TABLE, instance::surface::get_surface_hwnd, map::IntDashMap,
    renderer::VulkanRenderer,
};

/// Data associated with a [`vk::SwapchainKHR`].
pub struct SwapchainData {
    /// HWND of the surface the swapchain is tied to.
    pub hwnd: u32,

    /// Size of the swapchain images.
    pub image_size: (u32, u32),

    /// Format of the swapchain images.
    pub format: vk::Format,

    /// Vulkan overlay renderer
    pub(crate) renderer: Mutex<Option<VulkanRenderer>>,
}

/// [`vk::SwapchainKHR`] data mapping table.
static SWAPCHAIN_MAP: Lazy<IntDashMap<u64, SwapchainData>> = Lazy::new(IntDashMap::default);

/// Run a closure with [`SwapchainData`] data associated to a given [`vk::SwapchainKHR`].
#[must_use]
pub(super) fn with_swapchain_data<R>(
    swapchain: vk::SwapchainKHR,
    f: impl FnOnce(&SwapchainData) -> R,
) -> Option<R> {
    Some(f(&*SWAPCHAIN_MAP.get(&swapchain.as_raw())?))
}

/// Layer `vkCreateSwapchainKHR` implementation
pub(super) extern "system" fn create_swapchain(
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
    let hwnd = get_surface_hwnd(info.surface).unwrap();
    SWAPCHAIN_MAP.insert(
        swapchain,
        SwapchainData {
            hwnd,
            image_size: (info.image_extent.width, info.image_extent.height),
            format: info.image_format,
            renderer: Mutex::new(None),
        },
    );

    vk::Result::SUCCESS
}

/// Layer `vkDestroySwapchainKHR` implementation
pub(super) extern "system" fn destroy_swapchain(
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
    _ = with_swapchain_data(swapchain, |data| {
        debug!("vulkan renderer cleanup");
        data.renderer.lock().take();

        _ = Backends::with_backend(data.hwnd, |backend| {
            let mut render = backend.render.lock();
            render.invalidate_surface();
        });
    });
}
