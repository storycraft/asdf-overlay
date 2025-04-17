use core::{
    mem,
    num::NonZeroUsize,
    slice::{self},
};

use anyhow::Context;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::{
            Direct3D::{
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN, D3D_SRV_DIMENSION_TEXTURE2D,
                Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCOMPILE_WARNINGS_ARE_ERRORS, D3DCompile},
            },
            Direct3D11::{
                D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND_DESC,
                D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
                D3D11_BLEND_SRC_ALPHA, D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL,
                D3D11_CPU_ACCESS_WRITE, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA,
                D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, D3D11_RENDER_TARGET_BLEND_DESC,
                D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
                D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_SRV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
                D3D11_USAGE_DYNAMIC, D3D11_VIEWPORT, ID3D11Buffer, ID3D11Device,
                ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader,
                ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32G32_FLOAT},
                IDXGIKeyedMutex, IDXGISwapChain,
            },
        },
    },
    core::{BOOL, Interface, s},
};

const TEXTURE_SHADER: &str = include_str!("dx11/shaders/texture.hlsl");

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

const INPUT_DESC: [D3D11_INPUT_ELEMENT_DESC; 1] = [D3D11_INPUT_ELEMENT_DESC {
    SemanticName: s!("POSITION"),
    SemanticIndex: 0,
    Format: DXGI_FORMAT_R32G32_FLOAT,
    InputSlot: 0,
    AlignedByteOffset: 0,
    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
    InstanceDataStepRate: 0,
}];

enum TextureState {
    None,
    Handle(NonZeroUsize),
    Created {
        texture: ID3D11Texture2D,
        view: ID3D11ShaderResourceView,
    },
}

impl Drop for TextureState {
    fn drop(&mut self) {
        if let Self::Handle(handle) = self {
            unsafe { _ = CloseHandle(HANDLE(handle.get() as _)) };
        }
    }
}

pub struct Dx11Renderer {
    context: ID3D11DeviceContext,

    input_layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    constant_buffer: ID3D11Buffer,
    texture: TextureState,

    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    blend_state: windows::Win32::Graphics::Direct3D11::ID3D11BlendState,
}

impl Dx11Renderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
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

            let vs_blob_slice = slice::from_raw_parts::<u8>(
                vs_blob.GetBufferPointer() as _,
                vs_blob.GetBufferSize(),
            );

            let mut input_layout = None;
            device.CreateInputLayout(&INPUT_DESC, vs_blob_slice, Some(&mut input_layout))?;
            let input_layout = input_layout.context("failed to create input layout")?;

            let mut vertex_shader = None;
            device.CreateVertexShader(vs_blob_slice, None, Some(&mut vertex_shader))?;
            let vertex_shader = vertex_shader.context("vertex shader failed to link")?;

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

            let mut pixel_shader = None;
            device.CreatePixelShader(
                slice::from_raw_parts::<u8>(
                    ps_blob.GetBufferPointer() as _,
                    ps_blob.GetBufferSize(),
                ),
                None,
                Some(&mut pixel_shader),
            )?;
            let pixel_shader = pixel_shader.context("pixel shader failed to link")?;

            let mut context = None;
            device.CreateDeferredContext(0, Some(&mut context))?;
            let context = context.context("cannot create overlay context")?;

            let mut vertex_buffer = None;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: mem::size_of::<VertexArray>() as _,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as _,
                    CPUAccessFlags: 0,
                    MiscFlags: 0,
                    StructureByteStride: 0,
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: &VERTICES as *const _ as _,
                    SysMemPitch: 0,
                    SysMemSlicePitch: 0,
                }),
                Some(&mut vertex_buffer),
            )?;
            let vertex_buffer = vertex_buffer.context("cannot create vertex buffer")?;

            let mut constant_buffer = None;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: mem::size_of::<[f32; 4]>() as _,
                    Usage: D3D11_USAGE_DYNAMIC,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as _,
                    CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as _,
                    MiscFlags: 0,
                    StructureByteStride: 0,
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: &VERTICES as *const _ as _,
                    SysMemPitch: 0,
                    SysMemSlicePitch: 0,
                }),
                Some(&mut constant_buffer),
            )?;
            let constant_buffer = constant_buffer.context("cannot create vertex buffer")?;

            let mut blend_state = None;
            device.CreateBlendState(
                &D3D11_BLEND_DESC {
                    AlphaToCoverageEnable: BOOL(0),
                    IndependentBlendEnable: BOOL(0),
                    RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                        BlendEnable: BOOL(1),
                        SrcBlend: D3D11_BLEND_SRC_ALPHA,
                        DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                        BlendOp: D3D11_BLEND_OP_ADD,
                        SrcBlendAlpha: D3D11_BLEND_ONE,
                        DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                        BlendOpAlpha: D3D11_BLEND_OP_ADD,
                        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as _,
                    }; 8],
                },
                Some(&mut blend_state),
            )?;
            let blend_state = blend_state.context("cannot create blend state")?;

            Ok(Self {
                context,

                input_layout,
                vertex_buffer,
                constant_buffer,
                texture: TextureState::None,

                vertex_shader,
                pixel_shader,
                blend_state,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        let TextureState::Created { ref texture, .. } = self.texture else {
            return (0, 0);
        };

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };

        (desc.Width, desc.Height)
    }

    pub fn update_texture(&mut self, handle: Option<NonZeroUsize>) {
        let Some(handle) = handle else {
            self.texture = TextureState::None;
            return;
        };

        self.texture = TextureState::Handle(handle);
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &ID3D11Device,
        swapchain: &IDXGISwapChain,
        position: (f32, f32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let (texture, view) = match self.texture {
            TextureState::None => return Ok(()),

            TextureState::Handle(handle) => unsafe {
                self.texture = TextureState::None;
                let mut texture = None;
                if device
                    .OpenSharedResource::<ID3D11Texture2D>(HANDLE(handle.get() as _), &mut texture)
                    .is_err()
                {
                    self.texture = TextureState::None;
                }
                let texture = texture.context("failed to open shared texture")?;

                let mut view = None;
                device.CreateShaderResourceView(
                    &texture,
                    Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: 0,
                                MipLevels: 1,
                            },
                        },
                    }),
                    Some(&mut view),
                )?;
                let view = view.context("cannot create texture view")?;
                self.texture = TextureState::Created { texture, view };
                let TextureState::Created {
                    ref texture,
                    ref view,
                } = self.texture
                else {
                    unreachable!()
                };

                (texture, view)
            },

            TextureState::Created {
                ref texture,
                ref view,
            } => (texture, view),
        };

        let size = self.size();
        if size.0 == 0 || size.1 == 0 || screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let rect = [
            (position.0 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 / screen.1 as f32) * 2.0 + 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            -(size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            let mutex = texture.cast::<IDXGIKeyedMutex>()?;
            mutex.AcquireSync(0, u32::MAX)?;
            defer!({
                _ = mutex.ReleaseSync(0);
            });

            {
                let mut mapped_cbuffer = D3D11_MAPPED_SUBRESOURCE::default();
                self.context.Map(
                    &self.constant_buffer,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped_cbuffer),
                )?;
                mapped_cbuffer.pData.cast::<[f32; 4]>().write(rect);
                self.context.Unmap(&self.constant_buffer, 0);
            }

            let render_target = {
                let back_buffer = swapchain.GetBuffer::<ID3D11Texture2D>(0)?;

                let mut render_target = None;
                device.CreateRenderTargetView(&back_buffer, None, Some(&mut render_target))?;
                render_target.context("cannot create render target")?
            };

            self.context
                .OMSetRenderTargets(Some(&[Some(render_target)]), None);
            self.context
                .OMSetBlendState(&self.blend_state, None, 0x00ffffff);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: screen.0 as _,
                Height: screen.1 as _,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));

            self.context.IASetInputLayout(&self.input_layout);

            self.context.VSSetShader(&self.vertex_shader, None);
            self.context.PSSetShader(&self.pixel_shader, None);

            self.context
                .PSSetShaderResources(0, Some(&[Some(view.clone())]));

            self.context.IASetVertexBuffers(
                0,
                1,
                Some(&self.vertex_buffer as *const _ as _),
                Some(&(mem::size_of::<Vertex>() as _)),
                Some(&0),
            );
            self.context
                .VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));

            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN);
            self.context.Draw(4, 0);

            let mut command_list = None;
            self.context
                .FinishCommandList(false, Some(&mut command_list))?;
            let command_list = command_list.context("command list writing failed")?;

            device
                .GetImmediateContext()?
                .ExecuteCommandList(&command_list, true);
        }

        Ok(())
    }
}
