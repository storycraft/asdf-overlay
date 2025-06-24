mod frame;
mod shaders;

use core::mem;

use anyhow::Context;
use ash::{Device, vk};
use scopeguard::defer;
use shaders::{FRAGMENT_SHADER, VERTEX_SHADER};
use windows::{
    Win32::Graphics::{
        Direct3D11::{self, D3D11_TEXTURE2D_DESC},
        Dxgi::IDXGIResource,
    },
    core::Interface,
};

use crate::{renderer::vulkan::frame::FrameData, vulkan_layer::device::swapchain::SwapchainData};

pub struct VulkanRenderer {
    device: Device,

    descriptor_pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    texture_layout: vk::DescriptorSetLayout,
    texture_descriptor_set: vk::DescriptorSet,
    texture: Option<(vk::DeviceMemory, vk::Image, vk::ImageView)>,

    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,

    frame_datas: Vec<FrameData>,
}

impl VulkanRenderer {
    pub fn new(
        device: Device,
        queue_family_index: u32,
        swapchain_data: &SwapchainData,
        swapchain_images: &[vk::Image],
    ) -> anyhow::Result<Self> {
        let descriptor_pool = create_descriptor_pool(&device)?;
        let sampler = create_sampler(&device)?;
        let texture_layout = create_texture_layout(&device, sampler)?;
        let texture_descriptor_set =
            create_texture_descriptor_set(&device, descriptor_pool, texture_layout)?;

        let pipeline_layout = create_pipeline_layout(&device, texture_layout)?;
        let render_pass = create_render_pass(&device, swapchain_data.format)?;
        let pipeline = create_pipeline(
            &device,
            swapchain_data.image_size,
            pipeline_layout,
            render_pass,
        )?;
        let mut frame_data = Vec::with_capacity(swapchain_images.len());
        for &swapchain_image in swapchain_images {
            frame_data.push(
                FrameData::new(
                    &device,
                    queue_family_index,
                    render_pass,
                    swapchain_image,
                    swapchain_data.format,
                    swapchain_data.image_size,
                )
                .context("failed to create SwapchainImageData")?,
            );
        }

        Ok(Self {
            device,

            descriptor_pool,
            sampler,
            texture_layout,
            texture_descriptor_set,
            texture: None,

            pipeline_layout,
            render_pass,
            pipeline,

            frame_datas: frame_data,
        })
    }

    pub fn update_texture(
        &mut self,
        texture: Option<&Direct3D11::ID3D11Texture2D>,
        props: &vk::PhysicalDeviceMemoryProperties,
    ) -> anyhow::Result<()> {
        unsafe {
            if let Some((memory, image, view)) = self.texture.take() {
                self.device.device_wait_idle()?;

                self.device.destroy_image_view(view, None);
                self.device.destroy_image(image, None);
                self.device.free_memory(memory, None);
            }

            let Some(texture) = texture else {
                return Ok(());
            };

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            let handle = texture
                .cast::<IDXGIResource>()
                .unwrap()
                .GetSharedHandle()
                .unwrap();

            let mut external_memory_image_info = vk::ExternalMemoryImageCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE_KMT);
            let image = self.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::B8G8R8A8_UNORM)
                    .extent(vk::Extent3D {
                        width: desc.Width,
                        height: desc.Height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .push_next(&mut external_memory_image_info),
                None,
            )?;

            let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
            let requirements = {
                let mut requirements =
                    vk::MemoryRequirements2::default().push_next(&mut dedicated_requirements);
                self.device.get_image_memory_requirements2(
                    &vk::ImageMemoryRequirementsInfo2::default().image(image),
                    &mut requirements,
                );

                requirements.memory_requirements
            };

            let mut import_memory_info = vk::ImportMemoryWin32HandleInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE_KMT)
                .handle(handle.0 as _);
            let mut dedicated_alloc_info = vk::MemoryDedicatedAllocateInfo::default().image(image);

            if dedicated_requirements.prefers_dedicated_allocation == vk::TRUE
                || dedicated_requirements.requires_dedicated_allocation == vk::TRUE
            {
                import_memory_info.p_next = &mut dedicated_alloc_info as *const _ as _;
            }

            let memory_type_index = {
                let mut bits = requirements.memory_type_bits;

                props
                    .memory_types_as_slice()
                    .iter()
                    .enumerate()
                    .find_map({
                        |(index, ty)| {
                            if bits & 1 == 1
                                && ty
                                    .property_flags
                                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                            {
                                Some(index as u32)
                            } else {
                                bits >>= 1;
                                None
                            }
                        }
                    })
                    .unwrap_or_default()
            };

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type_index)
                .push_next(&mut import_memory_info);
            let memory = self.device.allocate_memory(&alloc_info, None)?;

            self.device.bind_image_memory(image, memory, 0)?;

            let view = self.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .format(vk::Format::B8G8R8A8_UNORM)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?;

            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let descriptor_writes = [vk::WriteDescriptorSet::default()
                .dst_set(self.texture_descriptor_set)
                .dst_binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_info)];
            self.device.update_descriptor_sets(&descriptor_writes, &[]);

            self.texture = Some((memory, image, view));
        }

        Ok(())
    }

    pub fn draw(
        &mut self,
        queue: vk::Queue,
        wait_semaphores: &[vk::Semaphore],
        index: u32,
        position: (i32, i32),
        size: (u32, u32),
        screen: (u32, u32),
    ) -> anyhow::Result<Option<vk::Semaphore>> {
        if self.texture.is_none() {
            return Ok(None);
        };

        let frame_data = self.frame_datas[index as usize];
        unsafe {
            self.device
                .wait_for_fences(&[frame_data.fence], true, u64::MAX)?;
            self.device
                .reset_command_pool(frame_data.command_pool, vk::CommandPoolResetFlags::empty())?;
            self.device.reset_fences(&[frame_data.fence])?;
        }

        let rect: [f32; 4] = [
            (position.0 as f32 / screen.0 as f32) * 2.0 - 1.0,
            (position.1 as f32 / screen.1 as f32) * 2.0 - 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            (size.1 as f32 / screen.1 as f32) * 2.0,
        ];
        let command_buffer = frame_data.command_buffer;
        unsafe {
            self.device.begin_command_buffer(
                command_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            let offset = vk::Offset2D {
                x: position.0.max(0),
                y: position.1.max(0),
            };
            self.device.cmd_begin_render_pass(
                command_buffer,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(frame_data.framebuffer)
                    .render_area(vk::Rect2D {
                        offset,
                        extent: vk::Extent2D {
                            width: size.0.min(screen.0 - offset.x as u32),
                            height: size.1.min(screen.1 - offset.y as u32),
                        },
                    }),
                vk::SubpassContents::INLINE,
            );

            let descriptor_sets = [self.texture_descriptor_set];
            self.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &descriptor_sets,
                &[],
            );

            self.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );

            self.device.cmd_push_constants(
                command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::cast_slice(&rect),
            );

            self.device.cmd_draw(command_buffer, 4, 1, 0, 0);

            self.device.cmd_end_render_pass(command_buffer);

            self.device.end_command_buffer(command_buffer)?;

            self.device.queue_submit(
                queue,
                &[vk::SubmitInfo::default()
                    .command_buffers(&[command_buffer])
                    .wait_semaphores(wait_semaphores)
                    .wait_dst_stage_mask(if wait_semaphores.is_empty() {
                        &[]
                    } else {
                        &[vk::PipelineStageFlags::TOP_OF_PIPE]
                    })
                    .signal_semaphores(&[frame_data.submit_semaphore])],
                frame_data.fence,
            )?;
        }

        Ok(Some(frame_data.submit_semaphore))
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().expect("failed to wait idle");

            for frame_data in &mut self.frame_datas {
                self.device
                    .destroy_semaphore(frame_data.submit_semaphore, None);
                self.device.destroy_fence(frame_data.fence, None);
                self.device
                    .free_command_buffers(frame_data.command_pool, &[frame_data.command_buffer]);
                self.device
                    .destroy_command_pool(frame_data.command_pool, None);
                self.device
                    .destroy_framebuffer(frame_data.framebuffer, None);
                self.device.destroy_image_view(frame_data.view, None);
            }

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);

            if let Some((memory, image, view)) = self.texture {
                self.device.destroy_image_view(view, None);
                self.device.destroy_image(image, None);
                self.device.free_memory(memory, None);
            }

            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.texture_layout, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

fn create_descriptor_pool(device: &Device) -> anyhow::Result<vk::DescriptorPool> {
    unsafe {
        device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&[vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .descriptor_count(1)]),
                None,
            )
            .context("failed to create DescriptorPool")
    }
}

fn create_texture_descriptor_set(
    device: &Device,
    descriptor_pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> anyhow::Result<vk::DescriptorSet> {
    unsafe {
        let mut set = vk::DescriptorSet::null();
        (device.fp_v1_0().allocate_descriptor_sets)(
            device.handle(),
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&[layout]),
            &mut set,
        )
        .result()
        .context("failed to create DescriptorSet")?;

        Ok(set)
    }
}

fn create_sampler(device: &Device) -> anyhow::Result<vk::Sampler> {
    unsafe {
        device
            .create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::NEAREST)
                    .min_filter(vk::Filter::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::REPEAT),
                None,
            )
            .context("failed to create Sampler")
    }
}

fn create_texture_layout(
    device: &Device,
    sampler: vk::Sampler,
) -> anyhow::Result<vk::DescriptorSetLayout> {
    unsafe {
        let samplers = [sampler];
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .immutable_samplers(&samplers)];

        device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
            .context("failed to create texture DescriptorSetLayout")
    }
}

fn create_render_pass(device: &Device, format: vk::Format) -> anyhow::Result<vk::RenderPass> {
    unsafe {
        let attachments = [vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let input_attachments = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&input_attachments)];

        device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses),
                None,
            )
            .context("failed to create RenderPass")
    }
}

fn create_pipeline_layout(
    device: &Device,
    texture_layout: vk::DescriptorSetLayout,
) -> anyhow::Result<vk::PipelineLayout> {
    unsafe {
        let set_layouts = [texture_layout];
        let push_constants_ranges = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: mem::size_of::<[f32; 4]>() as _,
        }];

        device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&push_constants_ranges),
                None,
            )
            .context("failed to create PipelineLayout")
    }
}

fn create_pipeline(
    device: &Device,
    size: (u32, u32),
    layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
) -> anyhow::Result<vk::Pipeline> {
    unsafe {
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
        let tessellation_state = vk::PipelineTessellationStateCreateInfo::default();

        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: size.0 as _,
            height: size.1 as _,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: size.0,
                height: size.1,
            },
        }];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);
        let rasterization_state =
            vk::PipelineRasterizationStateCreateInfo::default().line_width(1.0);
        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default();

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let color_blend_state =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default();

        let vertex_shader = device
            .create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(VERTEX_SHADER)),
                None,
            )
            .context("failed to create vertex shader module")?;
        defer!({
            device.destroy_shader_module(vertex_shader, None);
        });
        let fragment_shader = device
            .create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(FRAGMENT_SHADER)),
                None,
            )
            .context("failed to create fragment shader module")?;
        defer!({
            device.destroy_shader_module(fragment_shader, None);
        });

        let vertex_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .name(c"main");
        let fragment_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .name(c"main");

        let stages = [vertex_stage, fragment_stage];
        let mut pipeline = vk::Pipeline::null();
        (device.fp_v1_0().create_graphics_pipelines)(
            device.handle(),
            vk::PipelineCache::null(),
            1,
            &vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly_state)
                .tessellation_state(&tessellation_state)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization_state)
                .multisample_state(&multisample_state)
                .depth_stencil_state(&depth_stencil_state)
                .color_blend_state(&color_blend_state)
                .dynamic_state(&dynamic_state)
                .layout(layout)
                .render_pass(render_pass),
            0 as _,
            &mut pipeline,
        )
        .result()
        .context("failed to create Pipeline")?;
        Ok(pipeline)
    }
}
