use core::ptr::copy_nonoverlapping;

use anyhow::Context;
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC}};

use crate::util::wrap_com_manually_drop;

use super::{buffer::UploadBuffer, fence::FenceGuard};

pub struct OverlayTexture {
    size: (u32, u32),
    data: Vec<u8>,
    texture: Option<ID3D12Resource>,
    guard: FenceGuard,
}

impl OverlayTexture {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        let guard = FenceGuard::new(device)?;

        Ok(Self {
            size: (0, 0),
            data: Vec::new(),
            texture: None,
            guard,
        })
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

    pub fn prepare(
        &mut self,
        device: &ID3D12Device,
        queue: &ID3D12CommandQueue,
        command_list: &ID3D12GraphicsCommandList,
    ) -> anyhow::Result<()> {
        unsafe {
            let mut texture = None;
            device.CreateCommittedResource::<ID3D12Resource>(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
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
                },
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut texture,
            )?;
            let texture = texture.context("cannot create texture")?;

            let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
            let mut total_bytes = 0;
            let mut num_rows = 0;
            let mut row_byte_size = 0;
            device.GetCopyableFootprints(
                &texture.GetDesc(),
                0,
                1,
                0,
                Some(&mut footprint),
                Some(&mut num_rows),
                Some(&mut row_byte_size),
                Some(&mut total_bytes),
            );

            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: wrap_com_manually_drop(&texture),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };

            let upload = UploadBuffer::new(device, total_bytes)?;
            let ptr = upload.get_mapped_ptr().cast::<u8>();

            for row in 0..num_rows {
                let data_offset = row as usize * self.size.0 as usize * 4;
                copy_nonoverlapping(
                    self.data[data_offset..].as_ptr(),
                    ptr.offset(row_byte_size as isize * row as isize),
                    row_byte_size as _,
                );
            }
            upload.unmap();

            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: wrap_com_manually_drop(upload.buffer()),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: footprint,
                },
            };

            command_list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
        }

        Ok(())
    }
}
