use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::Direct3D12::*,
    System::Threading::{CreateEventA, WaitForSingleObject},
};

#[derive(Debug)]
pub struct RendererFence {
    fence: ID3D12Fence,
    event: HANDLE,
    fence_val: u64,
}

impl RendererFence {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            event: unsafe { CreateEventA(None, false, false, None)? },
            fence_val: 1,
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
                .SetEventOnCompletion(self.fence_val, self.event)?;
            WaitForSingleObject(self.event, u32::MAX);
        }
        Ok(())
    }
}

unsafe impl Send for RendererFence {}

impl Drop for RendererFence {
    fn drop(&mut self) {
        unsafe {
            _ = CloseHandle(self.event);
        }
    }
}
