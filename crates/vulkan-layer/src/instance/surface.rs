use asdf_overlay::surface::Surfaces;
use ash::vk::{self, Handle};
use once_cell::sync::Lazy;
use tracing::{debug, info, trace};

use crate::{instance::DISPATCH_TABLE, map::IntDashMap};

/// vkSurfaceKHR to HWND mapping table
static SURFACE_MAP: Lazy<IntDashMap<u64, u32>> = Lazy::new(IntDashMap::default);

/// Get the HWND associated with a given [`vk::SurfaceKHR`].`
pub fn get_surface_hwnd(surface: vk::SurfaceKHR) -> Option<u32> {
    SURFACE_MAP.get(&surface.as_raw()).map(|hwnd| *hwnd)
}

/// Layer `vkCreateWin32SurfaceKHR` implementation
pub(super) extern "system" fn create_win32_surface(
    instance: vk::Instance,
    create_info: *const vk::Win32SurfaceCreateInfoKHR,
    callback: *const vk::AllocationCallbacks,
    surface: *mut vk::SurfaceKHR,
) -> vk::Result {
    trace!("vkCreateWin32SurfaceKHR called");

    let res = unsafe {
        (DISPATCH_TABLE
            .get(&instance.as_raw())
            .unwrap()
            .create_win32_surface
            .unwrap())(instance, create_info, callback, surface)
    };
    if res != vk::Result::SUCCESS {
        return res;
    }

    info!("initializing vk surface data");
    let surface = unsafe { *surface }.as_raw();
    let hwnd = unsafe { *create_info }.hwnd as u32;
    debug!("registering vk surface: {surface} -> hwnd: {hwnd}");
    SURFACE_MAP.insert(surface, hwnd);

    vk::Result::SUCCESS
}

/// Layer `vkDestroySurfaceKHR` implementation
pub(super) extern "system" fn destroy_surface(
    instance: vk::Instance,
    surface: vk::SurfaceKHR,
    callback: *const vk::AllocationCallbacks,
) {
    trace!("vkDestroySurfaceKHR called");
    unsafe {
        (DISPATCH_TABLE
            .get(&instance.as_raw())
            .unwrap()
            .destroy_surface
            .unwrap())(instance, surface, callback);
    }
    info!("vulkan surface cleanup");

    SURFACE_MAP.remove(&surface.as_raw());
    Surfaces::cleanup_state(surface.as_raw());
}
