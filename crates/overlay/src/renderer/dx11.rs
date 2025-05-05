use core::{
    mem,
    num::NonZeroUsize,
    slice::{self},
};

use anyhow::Context;
use asdf_overlay_common::message::SharedHandle;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::{
            Direct3D::{
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN, D3D_SRV_DIMENSION_TEXTURE2D,
                Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCOMPILE_WARNINGS_ARE_ERRORS, D3DCompile},
            },
            Direct3D11::*,
            Dxgi::{Common::DXGI_FORMAT_R32G32_FLOAT, IDXGIKeyedMutex, IDXGISwapChain},
        },
    },
    core::{BOOL, Interface, s},
};

use crate::texture::OverlayTextureState;

use super::dx::TEXTURE_SHADER;

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

struct Dx11Tex {
    size: (u32, u32),
    _texture: ID3D11Texture2D,
    mutex: IDXGIKeyedMutex,
    view: ID3D11ShaderResourceView,
}

pub struct Dx11Renderer {
    input_layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    constant_buffer: ID3D11Buffer,
    texture: OverlayTextureState<Dx11Tex>,

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
                input_layout,
                vertex_buffer,
                constant_buffer,
                texture: OverlayTextureState::new(),

                vertex_shader,
                pixel_shader,
                blend_state,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.texture.map(|tex| tex.size).unwrap_or((0, 0))
    }

    pub fn update_texture(&mut self, shared: SharedHandle) {
        self.texture.update(shared);
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &ID3D11Device,
        cx: &ID3D11DeviceContext,
        swapchain: &IDXGISwapChain,
        position: (f32, f32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let Some(Dx11Tex {
            size, view, mutex, ..
        }) = self
            .texture
            .get_or_create(|handle| open_shared_texture(device, handle))?
        else {
            return Ok(());
        };

        let rect = [
            (position.0 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 / screen.1 as f32) * 2.0 + 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            -(size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            {
                let mut mapped_cbuffer = D3D11_MAPPED_SUBRESOURCE::default();
                cx.Map(
                    &self.constant_buffer,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped_cbuffer),
                )?;
                mapped_cbuffer.pData.cast::<[f32; 4]>().write(rect);
                cx.Unmap(&self.constant_buffer, 0);
            }

            let render_target = {
                let back_buffer = swapchain.GetBuffer::<ID3D11Texture2D>(0)?;

                let mut render_target = None;
                device.CreateRenderTargetView(&back_buffer, None, Some(&mut render_target))?;
                render_target.context("cannot create render target")?
            };

            cx.OMSetRenderTargets(Some(&[Some(render_target)]), None);
            cx.OMSetBlendState(&self.blend_state, None, 0x00ffffff);
            cx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: screen.0 as _,
                Height: screen.1 as _,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));

            cx.IASetInputLayout(&self.input_layout);

            cx.VSSetShader(&self.vertex_shader, None);
            cx.PSSetShader(&self.pixel_shader, None);

            mutex.AcquireSync(0, u32::MAX)?;
            defer!({
                _ = mutex.ReleaseSync(0);
            });

            cx.PSSetShaderResources(0, Some(&[Some(view.clone())]));

            cx.IASetVertexBuffers(
                0,
                1,
                Some(&self.vertex_buffer as *const _ as _),
                Some(&(mem::size_of::<Vertex>() as _)),
                Some(&0),
            );
            cx.VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));

            cx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLEFAN);
            cx.Draw(4, 0);
        }

        Ok(())
    }
}

fn open_shared_texture(
    device: &ID3D11Device,
    handle: NonZeroUsize,
) -> anyhow::Result<Option<Dx11Tex>> {
    let mut texture = None;
    if unsafe {
        device.OpenSharedResource::<ID3D11Texture2D>(HANDLE(handle.get() as _), &mut texture)
    }
    .is_err()
    {
        return Ok(None);
    }
    let texture = texture.context("failed to open shared texture")?;

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        texture.GetDesc(&mut desc);
    }

    let size = (desc.Width, desc.Height);
    if size.0 == 0 || size.1 == 0 {
        return Ok(None);
    }

    let mutex = texture.cast::<IDXGIKeyedMutex>()?;

    let mut view = None;
    unsafe {
        device.CreateShaderResourceView(
            &texture,
            Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: desc.Format,
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
    }
    let view = view.context("cannot create texture view")?;

    Ok(Some(Dx11Tex {
        size,
        _texture: texture,
        mutex,
        view,
    }))
}
