use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::Direct3D12::*,
    System::Threading::{CreateEventA, WaitForSingleObject},
};


pub struct FenceGuard {
    fence: ID3D12Fence,
    event: HANDLE,
    val: u64,
}

impl FenceGuard {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            event: unsafe { CreateEventA(None, false, false, None)? },
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

    pub fn wait(&self) -> anyhow::Result<()> {
        unsafe {
            self.fence.SetEventOnCompletion(self.val, self.event)?;
            WaitForSingleObject(self.event, u32::MAX);
        }

        Ok(())
    }
}

unsafe impl Send for FenceGuard {}

impl Drop for FenceGuard {
    fn drop(&mut self) {
        unsafe {
            _ = CloseHandle(self.event);
        }
    }
}
