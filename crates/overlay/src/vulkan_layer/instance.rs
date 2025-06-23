pub mod surface;

use core::{
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
};

use crate::{types::IntDashMap, vulkan_layer::device};

use super::{proc_table, resolve_proc};
use anyhow::Context;
use ash::vk::{self, BaseInStructure, Handle};
use once_cell::sync::Lazy;
use tracing::{debug, trace};

static DISPATCH_TABLE: Lazy<IntDashMap<u64, DispatchTable>> = Lazy::new(IntDashMap::default);

struct DispatchTable {
    get_proc_addr: vk::PFN_vkGetInstanceProcAddr,

    destroy_instance: vk::PFN_vkDestroyInstance,
    create_win32_surface: Option<vk::PFN_vkCreateWin32SurfaceKHR>,
    destroy_surface: Option<vk::PFN_vkDestroySurfaceKHR>,
}

impl DispatchTable {
    fn new(
        get_proc_addr: vk::PFN_vkGetInstanceProcAddr,
        instance: vk::Instance,
    ) -> anyhow::Result<Self> {
        macro_rules! proc {
            ($name:literal : $ty:ty) => {
                unsafe { resolve_proc!(get_proc_addr => instance, $name : $ty) }
            };
        }

        Ok(Self {
            destroy_instance: proc!(c"vkDestroyInstance": vk::PFN_vkDestroyInstance)
                .context("failed resolve instance fn vkDestroyInstance")?,
            create_win32_surface: proc!(c"vkCreateWin32SurfaceKHR": vk::PFN_vkCreateWin32SurfaceKHR),
            destroy_surface: proc!(c"vkDestroySurfaceKHR": vk::PFN_vkDestroySurfaceKHR),

            get_proc_addr,
        })
    }
}

#[tracing::instrument(skip(name))]
pub extern "system" fn get_proc_addr(
    instance: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let a = unsafe { &*CStr::from_ptr(name).to_string_lossy() };
    trace!("vkGetInstanceProcAddr called name: {}", a);

    unsafe {
        proc_table!(&*CStr::from_ptr(name).to_string_lossy() => {
            "vkGetInstanceProcAddr" => get_proc_addr: vk::PFN_vkGetInstanceProcAddr,
            "vkCreateInstance" => create_instance: vk::PFN_vkCreateInstance,
            "vkDestroyInstance" => destroy_instance: vk::PFN_vkDestroyInstance,
            "vkCreateDevice" => device::create_device: vk::PFN_vkCreateDevice,
            "vkCreateWin32SurfaceKHR" => surface::create_win32_surface: vk::PFN_vkCreateWin32SurfaceKHR,
            "vkDestroySurfaceKHR" => surface::destroy_surface: vk::PFN_vkDestroySurfaceKHR,
        });
    }

    unsafe { (DISPATCH_TABLE.get(&instance.as_raw())?.get_proc_addr)(instance, name) }
}

#[tracing::instrument]
extern "system" fn create_instance(
    info: *const vk::InstanceCreateInfo,
    callback: *const vk::AllocationCallbacks,
    instance: *mut vk::Instance,
) -> vk::Result {
    trace!("vkCreateInstance called");

    dbg!("finding");
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

    let Some(create_instance) = (unsafe {
        resolve_proc!(next_get_instance_proc_addr =>
            vk::Instance::null(),
            c"vkCreateInstance": vk::PFN_vkCreateInstance
        )
    }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };

    let res = unsafe { create_instance(info, callback, instance) };
    if res != vk::Result::SUCCESS {
        return res;
    }

    debug!("initializing instance dispatch table");
    let instance = unsafe { *instance };
    DISPATCH_TABLE.insert(
        instance.as_raw(),
        DispatchTable::new(next_get_instance_proc_addr, instance)
            .expect("failed to initialize dispatch table"),
    );

    vk::Result::SUCCESS
}

#[tracing::instrument]
extern "system" fn destroy_instance(
    instance: vk::Instance,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    trace!("vkDestroyInstance called");

    debug!("instance dispatch table cleanup");
    unsafe {
        (DISPATCH_TABLE
            .remove(&instance.as_raw())
            .unwrap()
            .1
            .destroy_instance)(instance, allocator)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct LayerInstanceLink {
    pub p_next: *mut LayerInstanceLink,
    pub pfn_next_get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub pfn_next_get_physical_device_proc_addr: vk::PFN_vkVoidFunction,
}

#[repr(C)]
#[derive(Copy, Clone)]
union LayerInstanceCreateInfoUnion {
    pub p_layer_info: *mut LayerInstanceLink,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct LayerInstanceCreateInfo {
    pub s_type: vk::StructureType,
    pub p_next: *mut c_void,
    pub function: i32,
    pub u: LayerInstanceCreateInfoUnion,
}

unsafe fn get_layer_link_info(
    instance_create_info: *const vk::InstanceCreateInfo,
) -> Option<NonNull<LayerInstanceCreateInfo>> {
    const VK_LAYER_LINK_INFO: i32 = 0;

    let mut layer_create_info: NonNull<BaseInStructure> =
        NonNull::new(instance_create_info.cast::<BaseInStructure>().cast_mut())?;
    loop {
        layer_create_info = NonNull::new(
            unsafe { layer_create_info.as_ref() }
                .p_next
                .cast::<BaseInStructure>()
                .cast_mut(),
        )?;

        if unsafe { layer_create_info.as_ref() }.s_type
            == vk::StructureType::LOADER_INSTANCE_CREATE_INFO
        {
            let layer_create_info = layer_create_info.cast::<LayerInstanceCreateInfo>();
            if unsafe { layer_create_info.as_ref() }.function == VK_LAYER_LINK_INFO {
                return Some(layer_create_info);
            }
        }
    }
}
