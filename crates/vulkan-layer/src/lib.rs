//! Vulkan layer implementation for providing vulkan overlay rendering support to [`asdf_overlay`].
//!
//! Vulkan overlay rendering cannot be solely done by hooking vulkan functions in the application,
//! as the required extensions cannot be enabled, and the surface window cannot be determined.
//!
//! A separated vulkan layer implementation is provided to selectively enable vulkan overlay rendering.

pub mod device;
pub mod instance;
mod map;

use core::ffi::c_void;

use ash::vk::{self, PFN_vkGetDeviceProcAddr, PFN_vkGetInstanceProcAddr, StructureType};

use tracing::{debug, trace};

/// Vulkan layer interface structure for negotiating the layer interface version and getting function pointers.
#[repr(C)]
struct VkNegotiateLayerInterface {
    /// Structure type, which is `VK_STRUCTURE_TYPE_LOADER_NEGOTIATE_LAYER_INTERFACE`.
    s_type: StructureType,

    /// Pointer to the next structure in a structure chain, or `NULL`.
    p_next: *const c_void,

    /// The version of the layer interface the layer is using.
    /// The loader will set this to the highest version it supports, and the layer can adjust
    /// its behavior accordingly.
    loader_layer_interface_version: u32,

    /// Function pointer to the layer's implementation of `vkGetInstanceProcAddr`.
    pfn_get_instance_proc_addr: Option<PFN_vkGetInstanceProcAddr>,

    /// Function pointer to the layer's implementation of `vkGetDeviceProcAddr`.
    pfn_get_device_proc_addr: Option<PFN_vkGetDeviceProcAddr>,

    /// Function pointer to the layer's implementation of `vkGetPhysicalDeviceProcAddr`.
    pfn_get_physical_device_proc_addr: Option<PFN_vkGetInstanceProcAddr>,
}

/// Entry point for the Vulkan loader to negotiate the layer interface version and get function pointers.
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

/// Cast a vulkan function pointer to `PFN_vkVoidFunction` for returning to the loader.
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

/// Resolve a vulkan function pointer by transmuting it to the desired type.
macro_rules! resolve_proc {
    ($f:expr => $this:expr, $name:literal : $ty:ty) => {
        ::core::mem::transmute::<::ash::vk::PFN_vkVoidFunction, Option<$ty>>($f(
            $this,
            $name.as_ptr(),
        ))
    };
}
use resolve_proc;
