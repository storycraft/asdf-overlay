use core::{mem, num::NonZeroU32};

use anyhow::Context;
use asdf_overlay_common::request::UpdateSharedHandle;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::{
            Direct3D::{D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP, D3D_SRV_DIMENSION_TEXTURE2D},
            Direct3D11::*,
            Dxgi::{Common::DXGI_FORMAT_R32G32_FLOAT, IDXGIKeyedMutex, IDXGIResource},
        },
    },
    core::{BOOL, Interface, s},
};

use crate::{renderer::dx::shaders, texture::OverlayTextureState};

#[derive(Clone, Copy)]
#[repr(C)]
struct Vertex {
    pub pos: (f32, f32),
}
type VertexArray = [Vertex; 4];
const VERTICES: VertexArray = [
    Vertex { pos: (0.0, 1.0) }, // bottom left
    Vertex { pos: (0.0, 0.0) }, // top left
    Vertex { pos: (1.0, 1.0) }, // bottom right
    Vertex { pos: (1.0, 0.0) }, // top right
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

const SAMPLER_DESC: D3D11_SAMPLER_DESC = D3D11_SAMPLER_DESC {
    Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
    AddressU: D3D11_TEXTURE_ADDRESS_BORDER,
    AddressV: D3D11_TEXTURE_ADDRESS_BORDER,
    AddressW: D3D11_TEXTURE_ADDRESS_BORDER,
    MipLODBias: 0.0,
    MaxAnisotropy: 0,
    ComparisonFunc: D3D11_COMPARISON_NEVER,
    BorderColor: [0.0; 4],
    MinLOD: 0.0,
    MaxLOD: D3D11_FLOAT32_MAX,
};

struct Dx11Tex {
    size: (u32, u32),
    texture: ID3D11Texture2D,
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
    blend_state: ID3D11BlendState,
    sampler_state: ID3D11SamplerState,
}

impl Dx11Renderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
            let mut input_layout = None;
            device
                .CreateInputLayout(&INPUT_DESC, shaders::VERTEX_SHADER, Some(&mut input_layout))
                .context("failed to create input layout")?;
            let input_layout = input_layout.unwrap();

            let mut vertex_shader = None;
            device
                .CreateVertexShader(shaders::VERTEX_SHADER, None, Some(&mut vertex_shader))
                .context("vertex shader failed to link")?;
            let vertex_shader = vertex_shader.unwrap();

            let mut pixel_shader = None;
            device
                .CreatePixelShader(shaders::PIXEL_SHADER, None, Some(&mut pixel_shader))
                .context("pixel shader failed to link")?;
            let pixel_shader = pixel_shader.unwrap();

            let mut vertex_buffer = None;
            device
                .CreateBuffer(
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
                )
                .context("cannot create vertex buffer")?;
            let vertex_buffer = vertex_buffer.unwrap();

            let mut constant_buffer = None;
            device
                .CreateBuffer(
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
                )
                .context("cannot create constant buffer")?;
            let constant_buffer = constant_buffer.unwrap();

            let mut blend_state = None;
            device
                .CreateBlendState(
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
                )
                .context("cannot create blend state")?;
            let blend_state = blend_state.unwrap();

            let mut sampler_state = None;
            device
                .CreateSamplerState(&SAMPLER_DESC, Some(&mut sampler_state))
                .context("cannot create blend state")?;
            let sampler_state = sampler_state.unwrap();

            Ok(Self {
                input_layout,
                vertex_buffer,
                constant_buffer,
                texture: OverlayTextureState::new(),

                vertex_shader,
                pixel_shader,
                blend_state,
                sampler_state,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.texture.map(|tex| tex.size).unwrap_or((0, 0))
    }

    pub fn update_texture(&mut self, shared: UpdateSharedHandle) {
        self.texture.update(shared);
    }

    pub fn take_texture(&mut self) -> Option<NonZeroU32> {
        self.texture.take_handle(|tex| unsafe {
            tex.texture
                .cast::<IDXGIResource>()
                .unwrap()
                .GetSharedHandle()
                .ok()
                .and_then(|handle| NonZeroU32::new(handle.0 as u32))
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &ID3D11Device,
        cx: &ID3D11DeviceContext,
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

            cx.PSSetSamplers(0, Some(&[Some(self.sampler_state.clone())]));

            cx.IASetVertexBuffers(
                0,
                1,
                Some(&self.vertex_buffer as *const _ as _),
                Some(&(mem::size_of::<Vertex>() as _)),
                Some(&0),
            );
            cx.VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));

            cx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);

            mutex.AcquireSync(0, u32::MAX)?;
            cx.PSSetShaderResources(0, Some(&[Some(view.clone())]));
            cx.Draw(4, 0);
            _ = mutex.ReleaseSync(0);
        }

        Ok(())
    }
}

fn open_shared_texture(
    device: &ID3D11Device,
    handle: NonZeroU32,
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
        texture,
        mutex,
        view,
    }))
}
