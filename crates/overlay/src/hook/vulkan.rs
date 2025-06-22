use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use ash::{
    Device, Entry, Instance,
    khr::swapchain,
    vk::{
        self, ApplicationInfo, InstanceCreateFlags, InstanceCreateInfo, PFN_vkQueuePresentKHR,
        PhysicalDeviceFeatures, PresentInfoKHR,
    },
};
use once_cell::sync::OnceCell;
use scopeguard::defer;
use tracing::{debug, error, trace};

struct Hook {
    queue_present_khr: DetourHook<QueuePresentKHRFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type QueuePresentKHRFn = PFN_vkQueuePresentKHR;

#[tracing::instrument]
pub fn hook() {
    fn inner() -> anyhow::Result<()> {
        let instance = create_dummy_instance().context("failed to create dummy vulkan instance")?;
        defer!(unsafe {
            instance.destroy_instance(None);
        });

        let device =
            create_dummy_device(&instance).context("failed to create dummy vulkan device")?;
        defer!(unsafe {
            device.destroy_device(None);
        });

        HOOK.get_or_try_init(|| unsafe {
            let swapchain_loader = swapchain::Device::new(&instance, &device);
            let fp = swapchain_loader.fp();

            debug!("hooking vkQueuePresentKHR");
            let queue_present_khr =
                DetourHook::attach(fp.queue_present_khr, hooked_queue_present_khr as _)?;

            Ok::<_, anyhow::Error>(Hook { queue_present_khr })
        })?;
        Ok(())
    }

    if let Err(err) = inner() {
        error!("failed to hook vulkan. err: {err:?}");
    }
}

#[tracing::instrument]
extern "system" fn hooked_queue_present_khr(
    this: vk::Queue,
    present_info: *const PresentInfoKHR,
) -> vk::Result {
    trace!("vkQueuePresentKHR called");

    let hook = HOOK.get().unwrap();
    unsafe { hook.queue_present_khr.original_fn()(this, present_info) }
}

// create dummy vulkan instance
fn create_dummy_instance() -> anyhow::Result<Instance> {
    let entry = unsafe { Entry::load().context("failed to load vulkan")? };

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
