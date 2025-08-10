use scopeguard::defer;
use windows::Win32::Graphics::{
    Direct3D11::*,
    Dxgi::{Common::DXGI_SAMPLE_DESC, IDXGIKeyedMutex},
};

use crate::util::with_keyed_mutex;

pub struct SharedHandleReader {
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
}

impl SharedHandleReader {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            staging: None,
            size: (0, 0),
        })
    }

    pub fn with_mapped<R>(
        &mut self,
        device: &ID3D11Device,
        mutex: Option<&IDXGIKeyedMutex>,
        cx: &ID3D11DeviceContext,
        src: &ID3D11Texture2D,
        size: (u32, u32),
        f: impl FnOnce(&D3D11_MAPPED_SUBRESOURCE) -> anyhow::Result<R>,
    ) -> anyhow::Result<Option<R>> {
        if size != self.size {
            self.staging.take();
        }

        unsafe {
            let staging = match self.staging {
                Some(ref staging) => staging,
                None => {
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    src.GetDesc(&mut desc);
                    if desc.Width == 0 || desc.Height == 0 {
                        return Ok(None);
                    }

                    let mut staging = None;
                    device.CreateTexture2D(
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

                    self.size = (desc.Width, desc.Height);
                    self.staging.insert(staging.unwrap())
                }
            };

            with_keyed_mutex(mutex, || {
                cx.CopyResource(staging, src);
            })?;

            {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                cx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                defer!({
                    cx.Unmap(staging, 0);
                });

                Ok(Some(f(&mapped)?))
            }
        }
    }
}
