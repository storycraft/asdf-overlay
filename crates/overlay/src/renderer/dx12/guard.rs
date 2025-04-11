use scopeguard::defer;
use windows::Win32::{
    Foundation::CloseHandle,
    Graphics::Direct3D12::*,
    System::Threading::{CreateEventA, WaitForSingleObject},
};

pub struct RendererGuard {
    fence: ID3D12Fence,
    val: u64,
}

impl RendererGuard {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            val: 0,
        })
    }

    pub fn queue(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        self.val += 1;
        unsafe {
            queue.Signal(&self.fence, self.val)?;
        }

        Ok(())
    }

    pub fn cleanup(&self) -> anyhow::Result<()> {
        unsafe {
            let event = CreateEventA(None, false, false, None)?;
            defer!({
                _ = CloseHandle(event);
            });
            self.fence.SetEventOnCompletion(self.val, event)?;
            WaitForSingleObject(event, u32::MAX);
        }

        Ok(())
    }
}
