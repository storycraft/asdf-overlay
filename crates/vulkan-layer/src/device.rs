pub mod queue;
pub mod swapchain;

use core::{
    ffi::{CStr, c_char, c_void},
    mem,
    ptr::NonNull,
    slice,
};

use crate::map::IntDashMap;

use super::{proc_table, resolve_proc};
use ash::{
    Device, khr,
    vk::{self, BaseInStructure, Handle},
};
use once_cell::sync::Lazy;
use tracing::{debug, trace};

/// Map of [`Device`] to its dispatch table.
static DISPATCH_TABLE: Lazy<IntDashMap<u64, DispatchTable>> = Lazy::new(IntDashMap::default);

/// Device dispatch table.
struct DispatchTable {
    /// Function pointer to next `vkGetDeviceProcAddr`.
    get_proc_addr: vk::PFN_vkGetDeviceProcAddr,

    /// Physical device the device was created from.
    physical_device: vk::PhysicalDevice,

    /// Queues created with the device.
    queues: Vec<vk::Queue>,

    /// Buffer of semaphores created for overlay rendering.
    semaphore_buf: Vec<vk::Semaphore>,

    /// Vulkan device handle.
    device: Device,

    /// [vk::SwapchainKHR] related [Device] functions. 
    swapchain_fn: khr::swapchain::DeviceFn,

    /// Function pointer to actual `vkQueuePresentKHR`.
    queue_present: Option<vk::PFN_vkQueuePresentKHR>,
}

impl DispatchTable {
    /// Create a new [`DispatchTable`].
    fn new(
        get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
        physical_device: vk::PhysicalDevice,
        raw_device: vk::Device,
        queues: Vec<vk::Queue>,
    ) -> Self {
        macro_rules! proc {
            ($name:literal : $ty:ty) => {
                unsafe { resolve_proc!(get_proc_addr => raw_device, $name : $ty) }
            };
        }

        let loader = |name: &CStr| unsafe {
            mem::transmute::<vk::PFN_vkVoidFunction, *const c_void>(get_proc_addr(
                raw_device,
                name.as_ptr(),
            ))
        };
        let device = unsafe { Device::load_with(loader, raw_device) };

        Self {
            physical_device,
            queues,
            semaphore_buf: vec![],

            device,
            swapchain_fn: khr::swapchain::DeviceFn::load(loader),
            queue_present: proc!(c"vkQueuePresentKHR": vk::PFN_vkQueuePresentKHR),

            get_proc_addr,
        }
    }
}

/// Data associated with a queue.
#[derive(Clone, Copy)]
pub struct QueueData {
    /// Vulkan device the queue was created from.
    pub device: vk::Device,

    /// Family index of the queue.
    pub family_index: u32,
}

/// Map of [vk::Queue] to its associated [QueueData].
static QUEUE_MAP: Lazy<IntDashMap<u64, QueueData>> = Lazy::new(IntDashMap::default);

/// Get the [`QueueData`] for a given [vk::Queue].
pub(super) fn get_queue_data(queue: vk::Queue) -> Option<QueueData> {
    QUEUE_MAP.get(&queue.as_raw()).map(|data| *data)
}

/// Layer's implementation of `vkGetDeviceProcAddr`.
#[tracing::instrument(skip(name))]
pub(super) extern "system" fn get_proc_addr(
    device: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let a = unsafe { &*CStr::from_ptr(name).to_string_lossy() };
    trace!("vkGetDeviceProcAddr called name: {}", a);

    unsafe {
        proc_table!(&*CStr::from_ptr(name).to_string_lossy() => {
            "vkGetDeviceProcAddr" => get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
            "vkDestroyDevice" => destroy_device: vk::PFN_vkDestroyDevice,
            "vkCreateSwapchainKHR" => swapchain::create_swapchain: vk::PFN_vkCreateSwapchainKHR,
            "vkDestroySwapchainKHR" => swapchain::destroy_swapchain: vk::PFN_vkDestroySwapchainKHR,
            "vkQueuePresentKHR" => queue::present: vk::PFN_vkQueuePresentKHR,
        });
    }

    unsafe { (DISPATCH_TABLE.get(&device.as_raw())?.get_proc_addr)(device, name) }
}

/// Implementation of layer's `vkCreateDevice`.
#[tracing::instrument]
pub(super) extern "system" fn create_device(
    ph_device: vk::PhysicalDevice,
    info: *const vk::DeviceCreateInfo,
    callback: *const vk::AllocationCallbacks,
    device: *mut vk::Device,
) -> vk::Result {
    trace!("vkCreateDevice called");

    let Some(layer_create_info) =
        (unsafe { get_layer_link_info(info).map(|mut info| info.as_mut()) })
    else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let link = unsafe { &*{ layer_create_info.u.p_layer_info } };
    // Move chain info for next layer
    layer_create_info.u.p_layer_info = unsafe { (*layer_create_info.u.p_layer_info).p_next };

    let Some(next_get_instance_proc_addr) = link.pfn_next_get_instance_proc_addr else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };

    let Some(create_device) = (unsafe {
        resolve_proc!(next_get_instance_proc_addr =>
            vk::Instance::null(),
            c"vkCreateDevice": vk::PFN_vkCreateDevice
        )
    }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };

    let Some(next_get_device_proc_addr) = link.pfn_next_get_device_proc_addr else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };

    let res = unsafe { create_device(ph_device, info, callback, device) };
    if res != vk::Result::SUCCESS {
        return res;
    }

    debug!("initializing device dispatch table");
    let info = unsafe { &*info };
    let device = unsafe { *device };

    let get_device_queue = (unsafe {
        resolve_proc!(next_get_device_proc_addr =>
            device,
            c"vkGetDeviceQueue": vk::PFN_vkGetDeviceQueue
        )
    })
    .unwrap();

    let mut queues = vec![];
    unsafe {
        for info in slice::from_raw_parts(
            info.p_queue_create_infos,
            info.queue_create_info_count as usize,
        ) {
            for i in 0..info.queue_count {
                let mut queue = vk::Queue::null();
                get_device_queue(device, info.queue_family_index, i, &mut queue);
                if queue != vk::Queue::null() {
                    debug!(
                        "found queue: {:?} family_index: {} index: {}",
                        queue, info.queue_family_index, i
                    );
                    queues.push(queue);
                    QUEUE_MAP.insert(
                        queue.as_raw(),
                        QueueData {
                            device,
                            family_index: info.queue_family_index,
                        },
                    );
                }
            }
        }
    }

    DISPATCH_TABLE.insert(
        device.as_raw(),
        DispatchTable::new(next_get_device_proc_addr, ph_device, device, queues),
    );

    vk::Result::SUCCESS
}

/// Implementation of layer's `vkDestroyDevice`.
#[tracing::instrument]
extern "system" fn destroy_device(
    device: vk::Device,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    trace!("vkDestroyDevice called");

    debug!("device dispatch table cleanup");
    let (_, table) = DISPATCH_TABLE.remove(&device.as_raw()).unwrap();
    for queue in table.queues {
        QUEUE_MAP.remove(&queue.as_raw());
    }

    unsafe { (table.device.fp_v1_0().destroy_device)(device, allocator) }
}

/// Vulkan structure for layer device link info.
#[repr(C)]
#[derive(Copy, Clone)]
struct VkLayerDeviceLink {
    /// Pointer to next layer's `VkLayerDeviceLink`.
    pub p_next: *mut VkLayerDeviceLink,

    /// Function pointer to next layer's `vkGetInstanceProcAddr`.
    pub pfn_next_get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr>,

    /// Function pointer to next layer's `vkGetDeviceProcAddr`.
    pub pfn_next_get_device_proc_addr: Option<vk::PFN_vkGetDeviceProcAddr>,
}

/// Vulkan structure for layer device create info.
#[repr(C)]
#[derive(Copy, Clone)]
union LayerDeviceCreateInfoUnion {
    /// Pointer to next layer's `VkLayerDeviceLink`.
    pub p_layer_info: *mut VkLayerDeviceLink,
}

/// Vulkan structure for layer device create info.
#[repr(C)]
#[derive(Copy, Clone)]
struct LayerDeviceCreateInfo {
    /// Structure type, which is [`vk::StructureType::LOADER_DEVICE_CREATE_INFO`].
    pub s_type: vk::StructureType,

    /// Pointer to next structure in the `p_next` chain.
    pub p_next: *mut c_void,

    /// Function indicator for this structure.
    pub function: i32,
    
    /// Union containing pointer to next layer's `VkLayerDeviceLink`.
    pub u: LayerDeviceCreateInfoUnion,
}

/// Helper to extract the [`LayerDeviceCreateInfo`] from a [`vk::DeviceCreateInfo`] pointer.
unsafe fn get_layer_link_info(
    device_create_info: *const vk::DeviceCreateInfo,
) -> Option<NonNull<LayerDeviceCreateInfo>> {
    const VK_LAYER_LINK_INFO: i32 = 0;

    let mut layer_create_info: NonNull<BaseInStructure> =
        NonNull::new(device_create_info.cast::<BaseInStructure>().cast_mut())?;
    loop {
        layer_create_info = NonNull::new(
            unsafe { layer_create_info.as_ref() }
                .p_next
                .cast::<BaseInStructure>()
                .cast_mut(),
        )?;

        if unsafe { layer_create_info.as_ref() }.s_type
            == vk::StructureType::LOADER_DEVICE_CREATE_INFO
        {
            let layer_create_info = layer_create_info.cast::<LayerDeviceCreateInfo>();
            if unsafe { layer_create_info.as_ref() }.function == VK_LAYER_LINK_INFO {
                return Some(layer_create_info);
            }
        }
    }
}
