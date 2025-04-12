use core::{ffi::c_void, ptr};

use anyhow::Context;
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::DXGI_SAMPLE_DESC};

#[derive(Debug)]
pub struct UploadBuffer {
    buffer: ID3D12Resource,
    ptr: *mut (),
}

impl UploadBuffer {
    pub fn new(device: &ID3D12Device, size: u64) -> anyhow::Result<UploadBuffer> {
        unsafe {
            let mut buffer = None;
            device.CreateCommittedResource::<ID3D12Resource>(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_UPLOAD,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: size,
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
                D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
                &mut buffer,
            )?;
            let buffer = buffer.context("cannot create buffer resource")?;

            let mut ptr = ptr::null_mut::<c_void>();
            buffer
                .Map(0, None, Some((&raw mut ptr).cast()))
                .context("cannot get memory location for upload buffer")?;

            Ok(UploadBuffer {
                buffer,
                ptr: ptr.cast(),
            })
        }
    }

    pub fn buffer(&self) -> &ID3D12Resource {
        &self.buffer
    }

    pub fn get_mapped_ptr(&self) -> *mut () {
        self.ptr
    }
}

impl Drop for UploadBuffer {
    fn drop(&mut self) {
        unsafe {
            self.buffer.Unmap(0, None);
        }
    }
}

unsafe impl Send for UploadBuffer {}
