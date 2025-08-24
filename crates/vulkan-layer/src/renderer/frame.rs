use anyhow::Context;
use ash::{
    Device,
    vk::{self, Handle},
};

#[derive(Clone, Copy)]
pub struct FrameData {
    pub command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
    pub view: vk::ImageView,
    pub framebuffer: vk::Framebuffer,
    pub fence: vk::Fence,
    pub submit_semaphore: vk::Semaphore,
}

impl FrameData {
    pub fn new(
        device: &Device,
        queue_family_index: u32,
        render_pass: vk::RenderPass,
        image: vk::Image,
        format: vk::Format,
        surface_size: (u32, u32),
    ) -> anyhow::Result<Self> {
        let command_pool = create_command_pool(device, queue_family_index)?;
        let command_buffer = create_command_buffer(device, command_pool)?;
        let view = create_image_view(device, image, format)?;
        let framebuffer = create_framebuffer(device, render_pass, surface_size, view)?;
        let fence = unsafe {
            device
                .create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
                .context("failed to create Fence")?
        };
        let submit_semaphore = unsafe {
            device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                .context("failed to create Semaphore")?
        };

        Ok(Self {
            command_pool,
            command_buffer,
            view,
            framebuffer,
            fence,
            submit_semaphore,
        })
    }
}

fn create_command_pool(
    device: &Device,
    queue_family_index: u32,
) -> anyhow::Result<vk::CommandPool> {
    unsafe {
        device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                    .queue_family_index(queue_family_index),
                None,
            )
            .context("failed to create CommandPool")
    }
}

fn create_command_buffer(
    device: &Device,
    pool: vk::CommandPool,
) -> anyhow::Result<vk::CommandBuffer> {
    unsafe {
        let mut buffer = vk::CommandBuffer::null();
        (device.fp_v1_0().allocate_command_buffers)(
            device.handle(),
            &vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(1)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(pool),
            &mut buffer,
        )
        .result()
        .context("failed to allocate CommandBuffer")?;

        // Dispatchable object in layer needs dispatch table initialization
        (buffer.as_raw() as *mut usize).write((device.handle().as_raw() as *mut usize).read());

        Ok(buffer)
    }
}

fn create_image_view(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
) -> anyhow::Result<vk::ImageView> {
    unsafe {
        device
            .create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )
            .context("failed to create ImageView")
    }
}

fn create_framebuffer(
    device: &Device,
    render_pass: vk::RenderPass,
    surface_size: (u32, u32),
    image_view: vk::ImageView,
) -> anyhow::Result<vk::Framebuffer> {
    unsafe {
        let framebuffer_attachments = [image_view];
        device
            .create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(render_pass)
                    .attachments(&framebuffer_attachments)
                    .width(surface_size.0)
                    .height(surface_size.1)
                    .layers(1),
                None,
            )
            .context("failed to create Framebuffer")
    }
}
