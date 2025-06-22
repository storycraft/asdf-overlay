use core::{mem::forget, slice};

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use ash::{
    Device, Entry, Instance,
    khr::swapchain,
    vk::{
        self, AcquireNextImageInfoKHR, AllocationCallbacks, ApplicationInfo, Fence, Handle,
        InstanceCreateFlags, InstanceCreateInfo, PFN_vkAcquireNextImage2KHR,
        PFN_vkAcquireNextImageKHR, PFN_vkDestroySwapchainKHR, PFN_vkQueuePresentKHR,
        PhysicalDeviceFeatures, PresentInfoKHR, Semaphore, SwapchainKHR,
    },
};
use once_cell::sync::{Lazy, OnceCell};
use scopeguard::defer;
use tracing::{debug, error, trace};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

use crate::types::IntDashMap;

struct Hook {
    queue_present_khr: DetourHook<QueuePresentKHRFn>,

    acquire_next_image_khr: DetourHook<AcquireNextImageKHRFn>,
    acquire_next_image2_khr: DetourHook<AcquireNextImage2KHRFn>,

    destroy_swapchain_khr: DetourHook<DestroySwapchainKHRFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type QueuePresentKHRFn = PFN_vkQueuePresentKHR;
type AcquireNextImageKHRFn = PFN_vkAcquireNextImageKHR;
type AcquireNextImage2KHRFn = PFN_vkAcquireNextImage2KHR;
type DestroySwapchainKHRFn = PFN_vkDestroySwapchainKHR;

// Swapchain -> vk::Device
static SWAPCHAIN_MAP: Lazy<IntDashMap<u64, vk::Device>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn hook() {
    fn inner() -> anyhow::Result<()> {
        let entry = unsafe { Entry::load().context("failed to load vulkan")? };

        let instance =
            create_dummy_instance(&entry).context("failed to create dummy vulkan instance")?;
        defer!(unsafe {
            instance.destroy_instance(None);
        });

        // Dropping entry free library
        forget(entry);

        let device =
            create_dummy_device(&instance).context("failed to create dummy vulkan device")?;
        defer!(unsafe {
            device.destroy_device(None);
        });

        let swapchain_loader = swapchain::Device::new(&instance, &device);
        let fp = swapchain_loader.fp();

        HOOK.get_or_try_init(|| unsafe {
            debug!("hooking vkDestroySwapchainKHR");
            let destroy_swapchain_khr =
                DetourHook::attach(fp.destroy_swapchain_khr, hooked_destroy_swapchain_khr as _)?;

            debug!("hooking vkAcquireNextImageKHR");
            let acquire_next_image_khr = DetourHook::attach(
                fp.acquire_next_image_khr,
                hooked_acquire_next_image_khr as _,
            )?;

            debug!("hooking vkAcquireNextImage2KHR");
            let acquire_next_image2_khr = DetourHook::attach(
                fp.acquire_next_image2_khr,
                hooked_acquire_next_image2_khr as _,
            )?;

            debug!("hooking vkQueuePresentKHR");
            let queue_present_khr =
                DetourHook::attach(fp.queue_present_khr, hooked_queue_present_khr)?;

            Ok::<_, anyhow::Error>(Hook {
                queue_present_khr,

                acquire_next_image_khr,
                acquire_next_image2_khr,

                destroy_swapchain_khr,
            })
        })?;
        Ok(())
    }

    if let Err(err) = inner() {
        error!("failed to hook vulkan. err: {err:?}");
    }
}

#[tracing::instrument]
extern "system" fn hooked_queue_present_khr(
    queue: vk::Queue,
    present_info: *const PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let info = unsafe { &*present_info };
    let swapchains = unsafe { slice::from_raw_parts(info.p_swapchains, info.swapchain_count as _) };
    for swapchain in swapchains {
        let Some((_, device)) = SWAPCHAIN_MAP.remove(&swapchain.as_raw()) else {
            continue;
        };

        debug!("swapchain: {swapchain:?} hwnd: {:?}", unsafe {
            GetActiveWindow()
        });
    }

    unsafe { HOOK.wait().queue_present_khr.original_fn()(queue, present_info) }
}

#[tracing::instrument]
extern "system" fn hooked_acquire_next_image_khr(
    device: vk::Device,
    swapchain: SwapchainKHR,
    timeout: u64,
    semaphore: Semaphore,
    fence: Fence,
    image_index: *mut u32,
) -> vk::Result {
    trace!("vkAcquireNextImageKHR called");

    SWAPCHAIN_MAP.insert(swapchain.as_raw(), device);

    unsafe {
        HOOK.wait().acquire_next_image_khr.original_fn()(
            device,
            swapchain,
            timeout,
            semaphore,
            fence,
            image_index,
        )
    }
}

#[tracing::instrument]
extern "system" fn hooked_acquire_next_image2_khr(
    device: vk::Device,
    acquire_info: *const AcquireNextImageInfoKHR,
    image_index: *mut u32,
) -> vk::Result {
    trace!("vkAcquireNextImage2KHR called");

    SWAPCHAIN_MAP.insert(unsafe { (*acquire_info).swapchain }.as_raw(), device);
    unsafe { HOOK.wait().acquire_next_image2_khr.original_fn()(device, acquire_info, image_index) }
}

#[tracing::instrument]
extern "system" fn hooked_destroy_swapchain_khr(
    device: vk::Device,
    swapchain: SwapchainKHR,
    p_allocator: *const AllocationCallbacks,
) {
    trace!("vkDestroySwapchainKHR called");

    if let Some((_, device)) = SWAPCHAIN_MAP.remove(&swapchain.as_raw()) {
        debug!("vulkan renderer cleanup");
    }

    unsafe { HOOK.wait().destroy_swapchain_khr.original_fn()(device, swapchain, p_allocator) }
}

// create dummy vulkan instance
fn create_dummy_instance(entry: &Entry) -> anyhow::Result<Instance> {
    let info = ApplicationInfo::default()
        .engine_version(0)
        .api_version(vk::make_api_version(0, 1, 1, 0));
    let create_info = InstanceCreateInfo::default()
        .application_info(&info)
        .flags(InstanceCreateFlags::default());

    unsafe {
        entry
            .create_instance(&create_info, None)
            .context("instance creation error")
    }
}

// create dummy vulkan device with first physical device with graphics queue
fn create_dummy_device(instance: &Instance) -> anyhow::Result<Device> {
    unsafe {
        let pdevices = instance
            .enumerate_physical_devices()
            .expect("physical device enumeration error");

        let (pdevice, queue_family_index) = pdevices
            .iter()
            .copied()
            .find_map(|pdevice| {
                instance
                    .get_physical_device_queue_family_properties(pdevice)
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        if info.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                            Some((pdevice, index))
                        } else {
                            None
                        }
                    })
            })
            .context("cannot find suitable device")?;
        let queue_family_index = queue_family_index as u32;

        {
            let queue_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&[1.0])];
            let extensions = [swapchain::NAME.as_ptr()];
            let features = PhysicalDeviceFeatures::default();

            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&extensions)
                .enabled_features(&features);

            instance.create_device(pdevice, &device_create_info, None)
        }
        .context("failed to create vulkan device")
    }
}
