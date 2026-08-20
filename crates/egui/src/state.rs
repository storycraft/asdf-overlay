use core::ptr;

use anyhow::Context as _;
use asdf_overlay::surface::{SharedTextureHandle, Surfaces};
use asdf_overlay_event::{GpuLuid, SurfaceInfo};
use egui::Context;
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
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
                D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11RenderTargetView, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
                CreateDXGIFactory1, DXGI_SHARED_RESOURCE_READ, IDXGIAdapter, IDXGIFactory1,
                IDXGIKeyedMutex, IDXGIResource1,
            },
        },
    },
    core::Interface as _,
};

pub struct SurfaceState {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub info: SurfaceInfo,

    d3d11_device: ID3D11Device,
    d3d11_cx: ID3D11DeviceContext,
    renderer: Renderer,
    surface_texture: (ID3D11Texture2D, IDXGIKeyedMutex, ID3D11RenderTargetView),
}

impl SurfaceState {
    pub(crate) fn new(id: u64, info: SurfaceInfo, width: u32, height: u32) -> anyhow::Result<Self> {
        let (d3d11_device, d3d11_cx) =
            create_device(info.gpu_id).context("creating d3d11 device")?;
        let renderer = Renderer::new(&d3d11_device).context("creating renderer")?;
        let surface_texture = create_surface_texture(&d3d11_device, width, height)
            .context("creating surface texture")?;

        let this = Self {
            id,
            width,
            height,
            info,

            d3d11_device,
            d3d11_cx,
            renderer,
            surface_texture,
        };

        this.update_surface();
        Ok(this)
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.on_resized();
    }

    pub(crate) fn render(
        &mut self,
        cx: &Context,
        renderer_output: RendererOutput,
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let (_, keyed_mutex, rtv) = &self.surface_texture;
        unsafe {
            keyed_mutex.AcquireSync(0, u32::MAX)?;
            defer!({
                _ = keyed_mutex.ReleaseSync(0);
            });

            self.d3d11_cx.ClearRenderTargetView(rtv, &clear_color);
            self.renderer
                .render(&self.d3d11_cx, rtv, cx, renderer_output)?;
        }

        Ok(())
    }

    fn on_resized(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        self.surface_texture = create_surface_texture(&self.d3d11_device, self.width, self.height)
            .expect("creating surface texture");

        self.update_surface();
    }

    fn update_surface(&self) {
        let shared_handle = unsafe {
            let res = self
                .surface_texture
                .0
                .cast::<IDXGIResource1>()
                .expect("cast to IDXGIResource1");
            let handle = res
                .CreateSharedHandle(None, DXGI_SHARED_RESOURCE_READ.0, None)
                .expect("creating shared texture");

            SharedTextureHandle::Nt(handle.0 as _)
        };

        if Surfaces::state(self.id, |state| {
            if let Err(err) = state.commit_overlay_texture(Some(shared_handle)) {
                error!("failed to commit overlay texture: {err:?}");
            }
        })
        .is_none()
        {
            error!("failed to commit overlay texture: surface not found");
        }
    }
}

fn create_surface_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> anyhow::Result<(ID3D11Texture2D, IDXGIKeyedMutex, ID3D11RenderTargetView)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0)
            as u32,
    };

    unsafe {
        let mut texture = None;
        device.CreateTexture2D(&desc, None, Some(&mut texture))?;
        let texture = texture.unwrap();

        let mut rtv = None;
        device.CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
        let rtv = rtv.unwrap();

        let keyed_mutex = texture.cast::<IDXGIKeyedMutex>()?;
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
