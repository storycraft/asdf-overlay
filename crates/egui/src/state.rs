use core::ptr;

use anyhow::{Context as _, bail};
use asdf_overlay::surface::{SharedTextureHandle, Surfaces};
use asdf_overlay_event::{GpuLuid, SurfaceInfo};
use egui_directx11::{Renderer, RendererOutput};
use scopeguard::defer;
use tracing::error;
use windows::{
    Win32::{
        Foundation::{HMODULE, LUID},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN},
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RESOURCE_MISC_SHARED,
                D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                CreateDXGIFactory1, DXGI_SHARED_RESOURCE_READ, IDXGIAdapter, IDXGIFactory1,
                IDXGIKeyedMutex, IDXGIResource, IDXGIResource1,
            },
        },
    },
    core::Interface as _,
};

pub struct State {
    pub egui_cx: egui::Context,
    d3d11_device: ID3D11Device,
    d3d11_cx: ID3D11DeviceContext,
    renderer: Renderer,
    surface_texture: (
        ID3D11Texture2D,
        Option<IDXGIKeyedMutex>,
        ID3D11RenderTargetView,
    ),
}

impl State {
    pub fn new(
        egui_cx: egui::Context,
        info: SurfaceInfo,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let (d3d11_device, d3d11_cx) =
            create_device(info.gpu_id).context("creating d3d11 device")?;
        let renderer = Renderer::new(&d3d11_device).context("creating renderer")?;
        let surface_texture =
            create_surface_texture(&d3d11_device, width, height, info.keyed_mutex)
                .context("creating surface texture")?;
        egui_cx.request_repaint();

        Ok(Self {
            egui_cx,
            d3d11_device,
            d3d11_cx,
            renderer,
            surface_texture,
        })
    }

    pub fn resize(&mut self, info: SurfaceInfo, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.surface_texture =
            create_surface_texture(&self.d3d11_device, width, height, info.keyed_mutex)
                .expect("creating surface texture");
        self.egui_cx.request_repaint();
    }

    pub fn commit_to_surface(&self, surface_id: u64) {
        let inner = || {
            let shared_handle = if self.surface_texture.1.is_some() {
                let res = self
                    .surface_texture
                    .0
                    .cast::<IDXGIResource1>()
                    .context("cast to IDXGIResource1")?;

                let handle =
                    unsafe { res.CreateSharedHandle(None, DXGI_SHARED_RESOURCE_READ.0, None) }
                        .context("creating shared texture")?;
                SharedTextureHandle::Nt(handle.0 as _)
            } else {
                let res = self
                    .surface_texture
                    .0
                    .cast::<IDXGIResource>()
                    .context("cast to IDXGIResource")?;

                SharedTextureHandle::Kmt(
                    unsafe { res.GetSharedHandle() }
                        .context("GetSharedHandle")?
                        .0 as _,
                )
            };

            let Some(res) = Surfaces::state(surface_id, |state| {
                state.commit_overlay_texture(Some(shared_handle))
            }) else {
                bail!("surface not found");
            };
            res.context("commit overlay texture")
        };

        if let Err(err) = inner() {
            error!("failed to commit overlay texture: {err:?}");
        }
    }

    pub fn render(
        &mut self,
        renderer_output: RendererOutput,
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let (_, keyed_mutex, rtv) = &self.surface_texture;
        let draw = || {
            unsafe {
                self.d3d11_cx.ClearRenderTargetView(rtv, &clear_color);
            }

            self.renderer
                .render(&self.d3d11_cx, rtv, &self.egui_cx, renderer_output)
        };

        if let Some(keyed_mutex) = keyed_mutex {
            unsafe {
                keyed_mutex.AcquireSync(0, u32::MAX)?;
            }
            defer!(unsafe {
                _ = keyed_mutex.ReleaseSync(0);
            });

            draw()?;
        } else {
            draw()?;

            unsafe {
                self.d3d11_cx.Flush();
            }
        }

        Ok(())
    }
}

fn create_surface_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    keyed_mutex: bool,
) -> anyhow::Result<(
    ID3D11Texture2D,
    Option<IDXGIKeyedMutex>,
    ID3D11RenderTargetView,
)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: if keyed_mutex {
            D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0
        } else {
            D3D11_RESOURCE_MISC_SHARED.0
        } as u32,
    };

    unsafe {
        let mut texture = None;
        device.CreateTexture2D(&desc, None, Some(&mut texture))?;
        let texture = texture.unwrap();

        let mut rtv = None;
        device.CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
        let rtv = rtv.unwrap();

        let keyed_mutex = texture.cast::<IDXGIKeyedMutex>().ok();
        Ok((texture, keyed_mutex, rtv))
    }
}

fn create_device(luid: GpuLuid) -> anyhow::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }?;
    let adapter = find_adapter_by_luid(
        &factory,
        LUID {
            LowPart: luid.low,
            HighPart: luid.high,
        },
    );

    let mut device = None;
    let mut cx = None;
    unsafe {
        D3D11CreateDevice(
            adapter.as_ref(),
            if adapter.is_none() {
                D3D_DRIVER_TYPE_HARDWARE
            } else {
                D3D_DRIVER_TYPE_UNKNOWN
            },
            HMODULE(ptr::null_mut()),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut cx),
        )?;
    };

    Ok((device.unwrap(), cx.unwrap()))
}

fn find_adapter_by_luid(factory: &IDXGIFactory1, luid: LUID) -> Option<IDXGIAdapter> {
    let mut i = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters(i) } {
        i += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc() }) else {
            continue;
        };

        if desc.AdapterLuid == luid {
            return Some(adapter);
        }
    }

    None
}
