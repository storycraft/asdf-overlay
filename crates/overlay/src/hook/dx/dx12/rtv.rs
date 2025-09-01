use windows::Win32::Graphics::{Direct3D12::*, Dxgi::IDXGISwapChain};

#[derive(Debug)]
pub struct RtvDescriptors {
    rtv_descriptor_heap: ID3D12DescriptorHeap,
    flags: u32,
    descriptor_size: usize,
}

impl RtvDescriptors {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        unsafe {
            let rtv_descriptor_heap = device.CreateDescriptorHeap::<ID3D12DescriptorHeap>(
                &D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                    NumDescriptors: D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT,
                    ..Default::default()
                },
            )?;
            let descriptor_size =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV) as usize;
            Ok(Self {
                rtv_descriptor_heap,
                flags: 0,
                descriptor_size,
            })
        }
    }

    pub fn reset(&mut self) {
        self.flags = 0;
    }

    pub fn with_next_swapchain<R>(
        &mut self,
        device: &ID3D12Device,
        swapchain: &IDXGISwapChain,
        index: usize,
        f: impl FnOnce(D3D12_CPU_DESCRIPTOR_HANDLE) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let desc = self.desc_for(index);
        if (self.flags >> index) & 1 != 1 {
            self.flags |= 1 << index;
            let backbuffer = unsafe { swapchain.GetBuffer::<ID3D12Resource>(index as _)? };
            unsafe { device.CreateRenderTargetView(&backbuffer, None, desc) };
        }

        f(desc)
    }

    fn desc_for(&self, backbuffer_index: usize) -> D3D12_CPU_DESCRIPTOR_HANDLE {
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
