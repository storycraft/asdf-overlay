use core::ptr;

use asdf_overlay_common::message::SharedHandle;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HANDLE, HMODULE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::{Common::DXGI_SAMPLE_DESC, IDXGIKeyedMutex},
        },
    },
    core::Interface,
};

use crate::texture::OverlayTextureState;

pub struct SharedHandleReader {
    device: ID3D11Device,
    cx: ID3D11DeviceContext,
    state: OverlayTextureState<StagingTex>,
}

impl SharedHandleReader {
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            let mut device = None;
            let mut cx = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )?;
            let device = device.unwrap();
            let cx = cx.unwrap();

            Ok(Self {
                device,
                cx,
                state: OverlayTextureState::new(),
            })
        }
    }

    pub fn update_shared(&mut self, shared: SharedHandle) {
        self.state.update(shared);
    }

    pub fn with_mapped<R>(
        &mut self,
        f: impl FnOnce((u32, u32), &D3D11_MAPPED_SUBRESOURCE) -> anyhow::Result<R>,
    ) -> anyhow::Result<Option<R>> {
        unsafe {
            let Some(StagingTex {
                size,
                src,
                mutex,
                staging,
            }) = self.state.get_or_create(|handle| {
                let mut src_texture = None;
                self.device.OpenSharedResource::<ID3D11Texture2D>(
                    HANDLE(handle.get() as _),
                    &mut src_texture,
                )?;
                let src = src_texture.unwrap();

                let mut desc = D3D11_TEXTURE2D_DESC::default();
                src.GetDesc(&mut desc);
                let size = (desc.Width, desc.Height);
                if desc.Width == 0 || desc.Height == 0 {
                    return Ok(None);
                }

                let mutex = src.cast::<IDXGIKeyedMutex>()?;

                let mut staging = None;
                self.device.CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: desc.Width,
                        Height: desc.Height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: desc.Format,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: 0,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as _,
                        MiscFlags: 0,
                    },
                    None,
                    Some(&mut staging),
                )?;
                let staging = staging.unwrap();

                Ok(Some(StagingTex {
                    size,
                    src,
                    mutex,
                    staging,
                }))
            })?
            else {
                return Ok(None);
            };

            {
                mutex.AcquireSync(0, u32::MAX)?;
                defer!({
                    _ = mutex.ReleaseSync(0);
                });

                self.cx.CopyResource(&*staging, &*src);
            }

            {
                let cx = &self.cx;
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                cx.Map(&*staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                defer!({
                    cx.Unmap(&*staging, 0);
                });

                Ok(Some(f(*size, &mapped)?))
            }
        }
    }
}

struct StagingTex {
    size: (u32, u32),
    src: ID3D11Texture2D,
    mutex: IDXGIKeyedMutex,
    staging: ID3D11Texture2D,
}
