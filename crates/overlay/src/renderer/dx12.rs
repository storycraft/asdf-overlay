mod sync;

use anyhow::Context;
use asdf_overlay_common::request::UpdateSharedHandle;
use core::{
    mem::ManuallyDrop,
    slice::{self},
};
use sync::RendererFence;
use windows::{
    Win32::{
        Foundation::{HANDLE, RECT},
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
            Direct3D12::*,
            Dxgi::{Common::DXGI_SAMPLE_DESC, IDXGISwapChain, IDXGISwapChain3},
        },
    },
    core::BOOL,
};

use crate::{
    hook::util::original_execute_command_lists, renderer::dx::shaders,
    texture::OverlayTextureState, util::wrap_com_manually_drop,
};

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

    pipeline: ID3D12PipelineState,
    texture: OverlayTextureState<ID3D12Resource>,
    texture_descriptor: ID3D12DescriptorHeap,

    command_list: [(ID3D12GraphicsCommandList, ID3D12CommandAllocator); MAX_RENDER_TARGETS],
    fence: RendererFence,
}

impl Dx12Renderer {
    #[tracing::instrument]
    pub fn new(
        device: &ID3D12Device,
        queue: &ID3D12CommandQueue,
        swapchain: &IDXGISwapChain,
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

            let mut pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: wrap_com_manually_drop(&sig),
                VS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: shaders::VERTEX_SHADER.as_ptr().cast(),
                    BytecodeLength: shaders::VERTEX_SHADER.len(),
                },
                PS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: shaders::PIXEL_SHADER.as_ptr().cast(),
                    BytecodeLength: shaders::PIXEL_SHADER.len(),
                },
                BlendState: D3D12_BLEND_DESC {
                    AlphaToCoverageEnable: BOOL(0),
                    IndependentBlendEnable: BOOL(0),
                    RenderTarget: [RENDER_TARGET_BLEND_DESC; 8],
                },
                RasterizerState: RASTERIZER_STATE,
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

            let command_list = array_util::try_from_fn(|_| {
                let command_alloc = device.CreateCommandAllocator::<ID3D12CommandAllocator>(
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                )?;

                let command_list = device.CreateCommandList::<_, _, ID3D12GraphicsCommandList>(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &command_alloc,
                    None,
                )?;
                command_list.Close()?;

                Ok::<_, anyhow::Error>((command_list, command_alloc))
            })?;

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

                pipeline,
                texture: OverlayTextureState::new(),
                texture_descriptor,

                command_list,
                fence: RendererFence::new(device)?,
            })
        }
    }

    pub fn update_texture(&mut self, shared: UpdateSharedHandle) {
        self.texture.update(shared);
    }

    #[tracing::instrument(skip(self))]
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &ID3D12Device,
        swapchain: &IDXGISwapChain3,
        backbuffer_index: u32,
        render_target: D3D12_CPU_DESCRIPTOR_HANDLE,
        queue: &ID3D12CommandQueue,
        position: (i32, i32),
        size: (u32, u32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        if self
            .texture
            .get_or_create(|handle| {
                let mut texture = None;
                unsafe {
                    device.OpenSharedHandle::<ID3D12Resource>(
                        HANDLE(handle.get() as _),
                        &mut texture,
                    )?;
                }
                let texture = texture.context("cannot open shared texture")?;

                let desc = unsafe { texture.GetDesc() };
                if desc.Width == 0 || desc.Height == 0 {
                    return Ok(None);
                }

                unsafe {
                    device.CreateShaderResourceView(
                        &texture,
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
                        self.texture_descriptor.GetCPUDescriptorHandleForHeapStart(),
                    );
                }

                Ok(Some(texture))
            })?
            .is_none()
        {
            return Ok(());
        };

        let rect: [f32; 4] = [
            (position.0 as f32 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 as f32 / screen.1 as f32) * 2.0 + 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            -(size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            let backbuffer = swapchain.GetBuffer::<ID3D12Resource>(backbuffer_index)?;
            let (ref command_list, ref command_alloc) =
                self.command_list[backbuffer_index as usize];

            command_alloc.Reset()?;
            command_list.Reset(command_alloc, &self.pipeline)?;

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

            command_list.OMSetRenderTargets(1, Some(&render_target), true, None);
            command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            command_list.DrawInstanced(4, 1, 0, 0);

            command_list.ResourceBarrier(&[transition(
                &backbuffer,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);

            command_list.Close()?;
            original_execute_command_lists(queue, &[Some(command_list.clone().into())]);
        }
        self.fence.register(queue)?;

        Ok(())
    }
}

impl Drop for Dx12Renderer {
    fn drop(&mut self) {
        self.fence.wait_pending().expect("error while waiting gpu");
    }
}

unsafe impl Send for Dx12Renderer {}
unsafe impl Sync for Dx12Renderer {}

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
