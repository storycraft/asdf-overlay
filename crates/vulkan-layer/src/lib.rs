pub mod device;
pub mod instance;
mod map;

use core::ffi::c_void;

use ash::vk::{self, PFN_vkGetDeviceProcAddr, PFN_vkGetInstanceProcAddr, StructureType};

use tracing::{debug, trace};

#[repr(C)]
struct VkNegotiateLayerInterface {
    s_type: StructureType,
    p_next: *const c_void,
    loader_layer_interface_version: u32,
    pfn_get_instance_proc_addr: Option<PFN_vkGetInstanceProcAddr>,
    pfn_get_device_proc_addr: Option<PFN_vkGetDeviceProcAddr>,
    pfn_get_physical_device_proc_addr: Option<PFN_vkGetInstanceProcAddr>,
}

#[tracing::instrument]
#[unsafe(export_name = "vkNegotiateLoaderLayerInterfaceVersion")]
extern "system" fn layer_negotiate_loader_layer_interface_version(
    version: *mut VkNegotiateLayerInterface,
) -> vk::Result {
    trace!("vkNegotiateLoaderLayerInterfaceVersion called");
    debug!("initializing vulkan layer");

    let version = unsafe { &mut *version };
    version.pfn_get_instance_proc_addr = Some(instance::get_proc_addr);
    version.pfn_get_device_proc_addr = Some(device::get_proc_addr);

    vk::Result::SUCCESS
}

macro_rules! proc_table {
    ($name:expr => {
        $($proc:literal => $func:path : $proc_ty:ty),* $(,)?
    }) => {
        match $name {
            $(
                $proc => return ::core::mem::transmute::<
                    $proc_ty,
                    ::ash::vk::PFN_vkVoidFunction
                >($func),
            )*
            _ => {}
        }
    };
}
use proc_table;

macro_rules! resolve_proc {
    ($f:expr => $this:expr, $name:literal : $ty:ty) => {
        ::core::mem::transmute::<::ash::vk::PFN_vkVoidFunction, Option<$ty>>($f(
            $this,
            $name.as_ptr(),
        ))
    };
}
use resolve_proc;
