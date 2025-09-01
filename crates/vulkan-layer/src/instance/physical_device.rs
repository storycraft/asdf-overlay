use ash::vk::{self, Handle};
use once_cell::sync::Lazy;

use crate::map::IntDashMap;

/// [`vk::PhysicalDevice`] to ([`vk::PhysicalDeviceMemoryProperties`]`, gpu luid) mapping table.
pub(super) static PHYSICAL_DEVICE_MAP: Lazy<
    IntDashMap<u64, (vk::PhysicalDeviceMemoryProperties, [u8; 8])>,
> = Lazy::new(IntDashMap::default);

/// Get the memory properties associated with a given [`vk::PhysicalDevice`].
pub fn get_physical_device_memory_properties(
    physical_device: vk::PhysicalDevice,
) -> Option<vk::PhysicalDeviceMemoryProperties> {
    PHYSICAL_DEVICE_MAP
        .get(&physical_device.as_raw())
        .map(|props| props.0)
}

///get the GPU LUID associated with a given [`vk::PhysicalDevice`].
pub fn get_physical_device_luid(physical_device: vk::PhysicalDevice) -> Option<[u8; 8]> {
    PHYSICAL_DEVICE_MAP
        .get(&physical_device.as_raw())
        .map(|props| props.1)
}
