mod buffer;
mod rtv;
mod sync;

use anyhow::Context;
use buffer::UploadBuffer;
use core::{
    mem::{self, ManuallyDrop},
    ptr::copy_nonoverlapping,
    slice::{self},
    str,
};
use rtv::RtvDescriptors;
use sync::RendererFence;
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct3D::{
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN,
                Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCOMPILE_WARNINGS_ARE_ERRORS, D3DCompile},
            },
            Direct3D12::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32G32_FLOAT, DXGI_SAMPLE_DESC},
                IDXGISwapChain, IDXGISwapChain3,
            },
        },
    },
    core::{BOOL, s},
};

use crate::{hook::call_original_execute_command_lists, util::wrap_com_manually_drop};

const TEXTURE_SHADER: &str = include_str!("dx12/shaders/texture.hlsl");

#[derive(Clone, Copy)]
#[repr(C)]
struct Vertex {
    pub pos: (f32, f32),
}
type VertexArray = [Vertex; 4];
const VERTICES: VertexArray = [
    Vertex { pos: (0.0, 0.0) },
    Vertex { pos: (1.0, 0.0) },
    Vertex { pos: (1.0, 1.0) },
    Vertex { pos: (0.0, 1.0) },
];

const INPUT_DESC: [D3D12_INPUT_ELEMENT_DESC; 1] = [D3D12_INPUT_ELEMENT_DESC {
    SemanticName: s!("POSITION"),
    SemanticIndex: 0,
    Format: DXGI_FORMAT_R32G32_FLOAT,
    InputSlot: 0,
    AlignedByteOffset: 0,
    InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
    InstanceDataStepRate: 0,
}];

const RENDER_TARGET_BLEND_DESC: D3D12_RENDER_TARGET_BLEND_DESC = D3D12_RENDER_TARGET_BLEND_DESC {
    BlendEnable: BOOL(1),
    SrcBlend: D3D12_BLEND_SRC_ALPHA,
    DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
    BlendOp: D3D12_BLEND_OP_ADD,
    SrcBlendAlpha: D3D12_BLEND_ONE,
    DestBlendAlpha: D3D12_BLEND_INV_SRC_ALPHA,
    BlendOpAlpha: D3D12_BLEND_OP_ADD,
    RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as _,
    LogicOpEnable: BOOL(0),
    LogicOp: D3D12_LOGIC_OP_NOOP,
};

const SAMPLER: D3D12_STATIC_SAMPLER_DESC = D3D12_STATIC_SAMPLER_DESC {
    Filter: D3D12_FILTER_MIN_MAG_MIP_POINT,
    AddressU: D3D12_TEXTURE_ADDRESS_MODE_BORDER,
    AddressV: D3D12_TEXTURE_ADDRESS_MODE_BORDER,
    AddressW: D3D12_TEXTURE_ADDRESS_MODE_BORDER,
    MipLODBias: 0.0,
    MaxAnisotropy: 0,
    ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
    BorderColor: D3D12_STATIC_BORDER_COLOR_TRANSPARENT_BLACK,
    MinLOD: 0.0,
    MaxLOD: D3D12_FLOAT32_MAX,
    ShaderRegister: 0,
    RegisterSpace: 0,
    ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
};

#[inline]
fn root_sig() -> D3D12_ROOT_SIGNATURE_DESC {
    D3D12_ROOT_SIGNATURE_DESC {
        NumParameters: 2,
        pParameters: [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                        Num32BitValues: 4,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &D3D12_DESCRIPTOR_RANGE {
                            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
                            NumDescriptors: 1,
                            BaseShaderRegister: 0,
                            RegisterSpace: 0,
                            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
                        },
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
        ]
        .as_ptr() as _,
        NumStaticSamplers: 1,
        pStaticSamplers: &SAMPLER,
        Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
            | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
            | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
            | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS,
    }
}

const RASTERIZER_STATE: D3D12_RASTERIZER_DESC = D3D12_RASTERIZER_DESC {
    FillMode: D3D12_FILL_MODE_SOLID,
    CullMode: D3D12_CULL_MODE_NONE,
    FrontCounterClockwise: BOOL(0),
    DepthBias: D3D12_DEFAULT_DEPTH_BIAS,
    DepthBiasClamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
    SlopeScaledDepthBias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
    DepthClipEnable: BOOL(0),
    MultisampleEnable: BOOL(0),
    AntialiasedLineEnable: BOOL(0),
    ForcedSampleCount: 0,
    ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
};

const MAX_RENDER_TARGETS: usize = D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT as _;

pub struct Dx12Renderer {
    sig: ID3D12RootSignature,
    rtv: RtvDescriptors,

    size: (u32, u32),
    data: Vec<u8>,

    pipeline: ID3D12PipelineState,
    vertex_buffer: ID3D12Resource,
    texture: Option<ID3D12Resource>,
    texture_descriptor: ID3D12DescriptorHeap,

    command_list: ID3D12GraphicsCommandList,
    command_alloc: ID3D12CommandAllocator,
    fence: RendererFence,
}

impl Dx12Renderer {
    #[tracing::instrument]
    pub fn new(
        device: &ID3D12Device,
        queue: &ID3D12CommandQueue,
        swapchain: &IDXGISwapChain3,
    ) -> anyhow::Result<Self> {
        unsafe {
            let swapchain_desc = swapchain.GetDesc()?;

            let mut sig = None;
            D3D12SerializeRootSignature(&root_sig(), D3D_ROOT_SIGNATURE_VERSION_1, &mut sig, None)?;
            let sig = sig.context("cannot create dx12 root signature")?;
            let sig = device.CreateRootSignature::<ID3D12RootSignature>(
                0,
                slice::from_raw_parts(sig.GetBufferPointer().cast::<u8>(), sig.GetBufferSize()),
            )?;

            let rtv = RtvDescriptors::new(device, swapchain)?;

            let mut vs_blob = None;
            D3DCompile(
                TEXTURE_SHADER.as_ptr() as _,
                TEXTURE_SHADER.len(),
                None,
                None,
                None,
                s!("vs_main"),
                s!("vs_5_0"),
                D3DCOMPILE_OPTIMIZATION_LEVEL3 | D3DCOMPILE_WARNINGS_ARE_ERRORS,
                0,
                &mut vs_blob,
                None,
            )?;
            let vs_blob = vs_blob.context("vertex shader failed to build")?;

            let mut ps_blob = None;
            D3DCompile(
                TEXTURE_SHADER.as_ptr() as _,
                TEXTURE_SHADER.len(),
                None,
                None,
                None,
                s!("ps_main"),
                s!("ps_5_0"),
                D3DCOMPILE_OPTIMIZATION_LEVEL3 | D3DCOMPILE_WARNINGS_ARE_ERRORS,
                0,
                &mut ps_blob,
                None,
            )?;
            let ps_blob = ps_blob.context("pixel shader failed to build")?;

            let mut pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: wrap_com_manually_drop(&sig),
                VS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: vs_blob.GetBufferPointer(),
                    BytecodeLength: vs_blob.GetBufferSize(),
                },
                PS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: ps_blob.GetBufferPointer(),
                    BytecodeLength: ps_blob.GetBufferSize(),
                },
                BlendState: D3D12_BLEND_DESC {
                    AlphaToCoverageEnable: BOOL(0),
                    IndependentBlendEnable: BOOL(0),
                    RenderTarget: [RENDER_TARGET_BLEND_DESC; 8],
                },
                RasterizerState: RASTERIZER_STATE,
                InputLayout: D3D12_INPUT_LAYOUT_DESC {
                    NumElements: INPUT_DESC.len() as _,
                    pInputElementDescs: INPUT_DESC.as_ptr(),
                },
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                NumRenderTargets: 1,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                SampleMask: u32::MAX,
                ..Default::default()
            };
            pipeline_desc.RTVFormats[0] = swapchain_desc.BufferDesc.Format;

            let pipeline =
                device.CreateGraphicsPipelineState::<ID3D12PipelineState>(&pipeline_desc)?;

            let command_alloc = device
                .CreateCommandAllocator::<ID3D12CommandAllocator>(D3D12_COMMAND_LIST_TYPE_DIRECT)?;

            let command_list = device.CreateCommandList::<_, _, ID3D12GraphicsCommandList>(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &command_alloc,
                None,
            )?;

            let mut fence = RendererFence::new(device)?;

            let mut vertex_buffer = None;
            device.CreateCommittedResource::<ID3D12Resource>(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: mem::size_of::<VertexArray>() as _,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut vertex_buffer,
            )?;
            let vertex_buffer = vertex_buffer.context("cannot create vertex buffer")?;

            init_vertex_buffer(device, queue, &mut fence, &command_list, &vertex_buffer)?;

            let texture_descriptor = device.CreateDescriptorHeap::<ID3D12DescriptorHeap>(
                &D3D12_DESCRIPTOR_HEAP_DESC {
                    NumDescriptors: 1,
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    ..Default::default()
                },
            )?;

            Ok(Self {
                sig,
                rtv,

                size: (0, 0),
                data: Vec::new(),

                pipeline,
                vertex_buffer,
                texture: None,
                texture_descriptor,

                command_list,
                command_alloc,
                fence,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn resize(&self, device: &ID3D12Device, swapchain: &IDXGISwapChain) {
        unsafe {
            self.rtv.reset(device, swapchain);
        }
    }

    pub fn update_texture(&mut self, width: u32, data: Vec<u8>) {
        if width == 0 || data.len() < width as _ {
            return;
        }

        let size = (width, (data.len() / width as usize / 4) as u32);

        self.size = size;
        self.data = data;
        self.texture.take();
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &ID3D12Device,
        swapchain: &IDXGISwapChain3,
        queue: &ID3D12CommandQueue,
        position: (f32, f32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if self.size.0 == 0 || self.size.1 == 0 || screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let rect: [f32; 4] = [
            (position.0 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 / screen.1 as f32) * 2.0 + 1.0,
            (self.size.0 as f32 / screen.0 as f32) * 2.0,
            -(self.size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            let backbuffer_index = swapchain.GetCurrentBackBufferIndex();

            let backbuffer = swapchain.GetBuffer::<ID3D12Resource>(backbuffer_index)?;
            let command_list = &self.command_list;

            self.command_alloc.Reset()?;
            command_list.Reset(&self.command_alloc, &self.pipeline)?;

            if self.texture.is_none() {
                let desc = D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                    Width: self.size.0 as _,
                    Height: self.size.1 as _,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    ..Default::default()
                };

                let mut texture = None;
                device.CreateCommittedResource::<ID3D12Resource>(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_DEFAULT,
                        ..Default::default()
                    },
                    D3D12_HEAP_FLAG_NONE,
                    &desc,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut texture,
                )?;
                let texture = texture.context("cannot create texture")?;
                upload_bgra_texture(
                    device,
                    queue,
                    &mut self.fence,
                    command_list,
                    &texture,
                    &self.texture_descriptor,
                    &self.data,
                )?;
                self.command_alloc.Reset()?;
                command_list.Reset(&self.command_alloc, &self.pipeline)?;

                self.texture = Some(texture);
            };

            command_list.SetGraphicsRootSignature(&self.sig);
            command_list.SetGraphicsRoot32BitConstants(0, 4, rect.as_ptr().cast(), 0);

            command_list.SetDescriptorHeaps(&[Some(self.texture_descriptor.clone())]);
            command_list.SetGraphicsRootDescriptorTable(
                1,
                self.texture_descriptor.GetGPUDescriptorHandleForHeapStart(),
            );

            command_list.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: screen.0 as _,
                Height: screen.1 as _,
                MinDepth: D3D12_MIN_DEPTH,
                MaxDepth: D3D12_MAX_DEPTH,
            }]);
            command_list.RSSetScissorRects(&[RECT {
                left: 0,
                top: 0,
                right: screen.0 as _,
                bottom: screen.1 as _,
            }]);

            command_list.ResourceBarrier(&[transition(
                &backbuffer,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )]);

            let render_target_desc = self.rtv.desc_for(backbuffer_index as _);
            command_list.OMSetRenderTargets(1, Some(&render_target_desc), true, None);
            command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN);
            command_list.IASetVertexBuffers(
                0,
                Some(&[D3D12_VERTEX_BUFFER_VIEW {
                    BufferLocation: self.vertex_buffer.GetGPUVirtualAddress(),
                    SizeInBytes: mem::size_of::<VertexArray>() as _,
                    StrideInBytes: mem::size_of::<Vertex>() as _,
                }]),
            );

            command_list.DrawInstanced(4, 1, 0, 0);

            command_list.ResourceBarrier(&[transition(
                &backbuffer,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);

            command_list.Close()?;

            call_original_execute_command_lists(queue, &[Some(command_list.clone().into())]);
        }
        self.fence.register(queue)?;
        self.fence.wait_pending()?;

        Ok(())
    }
}

impl Drop for Dx12Renderer {
    fn drop(&mut self) {
        self.fence.wait_pending().expect("error while waiting gpu");
    }
}

unsafe fn transition(
    res: &ID3D12Resource,
    from: D3D12_RESOURCE_STATES,
    to: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: unsafe { wrap_com_manually_drop(res) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: from,
                StateAfter: to,
            }),
        },
    }
}

unsafe fn init_vertex_buffer(
    device: &ID3D12Device,
    queue: &ID3D12CommandQueue,
    fence: &mut RendererFence,
    command_list: &ID3D12GraphicsCommandList,
    vertex_buffer: &ID3D12Resource,
) -> anyhow::Result<()> {
    unsafe {
        let upload = UploadBuffer::new(device, mem::size_of::<VertexArray>() as _)?;
        upload
            .get_mapped_ptr()
            .cast::<VertexArray>()
            .write(VERTICES);

        command_list.CopyResource(vertex_buffer, upload.buffer());
        command_list.ResourceBarrier(&[transition(
            vertex_buffer,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER,
        )]);

        command_list.Close()?;
        call_original_execute_command_lists(queue, &[Some(command_list.clone().into())]);
        fence.register(queue)?;
        fence.wait_pending()?;
    }

    Ok(())
}

// todo optimization
unsafe fn upload_bgra_texture(
    device: &ID3D12Device,
    queue: &ID3D12CommandQueue,
    fence: &mut RendererFence,
    command_list: &ID3D12GraphicsCommandList,
    texture: &ID3D12Resource,
    texture_descriptor: &ID3D12DescriptorHeap,
    data: &[u8],
) -> anyhow::Result<()> {
    unsafe {
        let desc = texture.GetDesc();
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut total_bytes = 0;
        let mut num_rows = 0;
        let mut row_byte_size = 0;
        device.GetCopyableFootprints(
            &desc,
            0,
            1,
            0,
            Some(&mut footprint),
            Some(&mut num_rows),
            Some(&mut row_byte_size),
            Some(&mut total_bytes),
        );

        let dst = D3D12_TEXTURE_COPY_LOCATION {
            pResource: wrap_com_manually_drop(texture),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };

        let upload = UploadBuffer::new(device, total_bytes)?;
        let ptr = upload.get_mapped_ptr().cast::<u8>();

        let pitch = footprint.Footprint.RowPitch as usize;
        for y in 0..num_rows as usize {
            let data_offset = y * row_byte_size as usize;
            copy_nonoverlapping::<u8>(
                data[data_offset..].as_ptr(),
                ptr.byte_add(pitch * y),
                row_byte_size as _,
            );
        }

        let src = D3D12_TEXTURE_COPY_LOCATION {
            pResource: wrap_com_manually_drop(upload.buffer()),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: footprint,
            },
        };

        command_list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);

        command_list.ResourceBarrier(&[transition(
            texture,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        )]);

        device.CreateShaderResourceView(
            texture,
            Some(&D3D12_SHADER_RESOURCE_VIEW_DESC {
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Format: desc.Format,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_SRV {
                        MipLevels: 1,
                        ..Default::default()
                    },
                },
            }),
            texture_descriptor.GetCPUDescriptorHandleForHeapStart(),
        );

        command_list.Close()?;
        call_original_execute_command_lists(queue, &[Some(command_list.clone().into())]);
        fence.register(queue)?;
        fence.wait_pending()?;
    }

    Ok(())
}
