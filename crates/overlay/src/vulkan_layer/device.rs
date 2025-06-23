mod queue;
mod swapchain;

use core::{
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
    slice,
};

use crate::types::IntDashMap;

use super::{proc_table, resolve_proc};
use anyhow::Context;
use ash::vk::{self, BaseInStructure, Handle};
use once_cell::sync::Lazy;
use tracing::{debug, trace};

static DISPATCH_TABLE: Lazy<IntDashMap<u64, DispatchTable>> = Lazy::new(IntDashMap::default);

struct DispatchTable {
    get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    queues: Vec<vk::Queue>,

    destroy_device: vk::PFN_vkDestroyDevice,
    create_swapchain: Option<vk::PFN_vkCreateSwapchainKHR>,
    destroy_swapchain: Option<vk::PFN_vkDestroySwapchainKHR>,
    queue_present: Option<vk::PFN_vkQueuePresentKHR>,
}

impl DispatchTable {
    fn new(
        get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
        device: vk::Device,
        queues: Vec<vk::Queue>,
    ) -> anyhow::Result<Self> {
        macro_rules! proc {
            ($name:literal : $ty:ty) => {
                unsafe { resolve_proc!(get_proc_addr => device, $name : $ty) }
            };
        }

        Ok(Self {
            queues,

            destroy_device: proc!(c"vkDestroyDevice": vk::PFN_vkDestroyDevice)
                .context("failed to resolve device fn vkDestroyDevice")?,
            create_swapchain: proc!(c"vkCreateSwapchainKHR": vk::PFN_vkCreateSwapchainKHR),
            destroy_swapchain: proc!(c"vkDestroySwapchainKHR": vk::PFN_vkDestroySwapchainKHR),
            queue_present: proc!(c"vkQueuePresentKHR": vk::PFN_vkQueuePresentKHR),

            get_proc_addr,
        })
    }
}

// Queue -> Device
static QUEUE_MAP: Lazy<IntDashMap<u64, vk::Device>> = Lazy::new(IntDashMap::default);

pub fn get_queue_device(queue: vk::Queue) -> Option<vk::Device> {
    QUEUE_MAP.get(&queue.as_raw()).map(|device| *device)
}

#[tracing::instrument(skip(name))]
pub extern "system" fn get_proc_addr(
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

#[tracing::instrument]
pub extern "system" fn create_device(
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
                }
            }
        }
    }
    for queue in &queues {
        QUEUE_MAP.insert(queue.as_raw(), device);
    }

    DISPATCH_TABLE.insert(
        device.as_raw(),
        DispatchTable::new(next_get_device_proc_addr, device, queues)
            .expect("failed to initialize dispatch table"),
    );

    vk::Result::SUCCESS
}

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

    unsafe { (table.destroy_device)(device, allocator) }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VkLayerDeviceLink {
    pub p_next: *mut VkLayerDeviceLink,
    pub pfn_next_get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub pfn_next_get_device_proc_addr: Option<vk::PFN_vkGetDeviceProcAddr>,
}

#[repr(C)]
#[derive(Copy, Clone)]
union LayerDeviceCreateInfoUnion {
    pub p_layer_info: *mut VkLayerDeviceLink,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct LayerDeviceCreateInfo {
    pub s_type: vk::StructureType,
    pub p_next: *mut c_void,
    pub function: i32,
    pub u: LayerDeviceCreateInfoUnion,
}

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
