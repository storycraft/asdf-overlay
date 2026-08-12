use asdf_overlay::{event_sink::OverlayEventSink, surface::Surfaces};
use asdf_overlay_event::{Event, SurfaceEvent};
use ash::vk::{self, AllocationCallbacks, Handle};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tracing::trace;

use crate::{device::DISPATCH_TABLE, map::IntDashMap, renderer::VulkanRenderer};

/// Data associated with a [`vk::SwapchainKHR`].
pub struct SwapchainData {
    /// Surface identifier
    pub surface: u64,

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

    let swapchain = unsafe { *swapchain }.as_raw();

    SWAPCHAIN_MAP.insert(
        swapchain,
        SwapchainData {
            surface: info.surface.as_raw(),
            image_size: (info.image_extent.width, info.image_extent.height),
            format: info.image_format,
            renderer: Mutex::new(None),
        },
    );

    let id = info.surface.as_raw();
    Surfaces::with_get(id, |state| {
        let extent = info.image_extent;

        state.resize(extent.width, extent.height);
        OverlayEventSink::emit(Event::Surface {
            id,
            event: SurfaceEvent::Resized {
                width: extent.width,
                height: extent.height,
            },
        });
    });

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
    let Some((_, data)) = SWAPCHAIN_MAP.remove(&swapchain.as_raw()) else {
        return;
    };

    Surfaces::with_get(data.surface, |state| {
        state.texture.invalidate();
    });
}
