mod shaders;

use core::mem;

use anyhow::Context;
use ash::{Device, vk};
use scopeguard::defer;
use shaders::{FRAGMENT_SHADER, VERTEX_SHADER};

pub struct VulkanRenderer {
    device: Device,

    command_pool: vk::CommandPool,

    sampler: vk::Sampler,
    texture_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    fence: vk::Fence,
}

impl VulkanRenderer {
    pub fn new(
        device: Device,
        queue_family_index: u32,
        format: vk::Format,
    ) -> anyhow::Result<Self> {
        unsafe {
            let command_pool = create_command_pool(&device, queue_family_index)?;
            let sampler = create_sampler(&device)?;
            let texture_layout = create_texture_layout(&device, sampler)?;
            let pipeline_layout = create_pipeline_layout(&device, texture_layout)?;
            let render_pass = create_render_pass(&device, format)?;
            let pipeline = create_pipeline(&device, pipeline_layout, render_pass)?;
            let fence = device
                .create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
                .context("failed to create Fence")?;

            Ok(Self {
                device,

                command_pool,

                sampler,
                texture_layout,
                pipeline_layout,
                render_pass,
                pipeline,
                fence,
            })
        }
    }

    pub fn draw(
        &mut self,
        queue: vk::Queue,
        index: u32,
        position: (i32, i32),
        size: (u32, u32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        unsafe {
            // let command_buffer = 0;

            // let command_buffers = [command_buffer];
            // self.device
            //     .queue_submit(
            //         queue,
            //         &[vk::SubmitInfo::default().command_buffers(command_buffers)],
            //         self.fence,
            //     )
            //     .context("queue submit failed")?;
        }
        Ok(())
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            _ = self.device.wait_for_fences(&[self.fence], true, u64::MAX);
            self.device.destroy_fence(self.fence, None);
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

fn create_command_pool(
    device: &Device,
    queue_family_index: u32,
) -> anyhow::Result<vk::CommandPool> {
    unsafe {
        device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(queue_family_index),
                None,
            )
            .context("failed to create CommandPool")
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
            .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
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
    layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
) -> anyhow::Result<vk::Pipeline> {
    unsafe {
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
        let tessellation_state = vk::PipelineTessellationStateCreateInfo::default();
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
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
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)];
        let color_blend_state =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

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
