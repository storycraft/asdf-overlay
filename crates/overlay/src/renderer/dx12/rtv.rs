use windows::Win32::Graphics::{Direct3D12::*, Dxgi::IDXGISwapChain};

use super::MAX_RENDER_TARGETS;

pub struct RtvDescriptors {
    rtv_descriptor_heap: ID3D12DescriptorHeap,
    descriptor_size: usize,
}

impl RtvDescriptors {
    pub fn new(device: &ID3D12Device, swapchain: &IDXGISwapChain) -> anyhow::Result<Self> {
        unsafe {
            let rtv_descriptor_heap = device.CreateDescriptorHeap::<ID3D12DescriptorHeap>(
                &D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                    NumDescriptors: MAX_RENDER_TARGETS as _,
                    ..Default::default()
                },
            )?;
            let descriptor_size =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
            let this = Self {
                rtv_descriptor_heap,
                descriptor_size,
            };
            this.reset(device, swapchain);

            Ok(this)
        }
    }

    pub unsafe fn reset(&self, device: &ID3D12Device, swapchain: &IDXGISwapChain) {
        for i in 0..MAX_RENDER_TARGETS {
            unsafe {
                let Ok(ref backbuffer) = swapchain.GetBuffer::<ID3D12Resource>(i as _) else {
                    break;
                };

                device.CreateRenderTargetView(backbuffer, None, self.desc_for(i));
            }
        }
    }

    pub unsafe fn desc_for(&self, backbuffer_index: usize) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        unsafe {
            D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: self
                    .rtv_descriptor_heap
                    .GetCPUDescriptorHandleForHeapStart()
                    .ptr
                    + self.descriptor_size * backbuffer_index,
            }
        }
    }
}
