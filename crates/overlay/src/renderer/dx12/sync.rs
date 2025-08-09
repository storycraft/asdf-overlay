use windows::Win32::{Foundation::HANDLE, Graphics::Direct3D12::*};

#[derive(Debug)]
pub struct RendererFence {
    fence: ID3D12Fence,
    fence_val: u64,
}

impl RendererFence {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            fence_val: 0,
        })
    }

    pub fn register(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        self.fence_val += 1;
        unsafe {
            queue.Signal(&self.fence, self.fence_val)?;
        }

        Ok(())
    }

    pub fn wait_pending(&self) -> anyhow::Result<()> {
        unsafe {
            self.fence
                .SetEventOnCompletion(self.fence_val, HANDLE(0 as _))?;
        }
        Ok(())
    }
}

unsafe impl Send for RendererFence {}
