use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::Direct3D12::*,
    System::Threading::{CreateEventA, WaitForSingleObject},
};

use super::MAX_RENDER_TARGETS;

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

    pub fn wait(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        self.val += 1;
        unsafe {
            queue.Signal(&self.fence, self.val)?;
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

pub struct RendererGuard {
    fence: ID3D12Fence,
    event: HANDLE,
    next: u64,
    registered: bool,
}

impl RendererGuard {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            event: unsafe { CreateEventA(None, false, false, None)? },
            next: 0,
            registered: false,
        })
    }

    pub fn register(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        unsafe {
            queue.Signal(&self.fence, self.next)?;
        }

        if !self.registered {
            self.registered = true;
        }

        Ok(())
    }

    pub fn wait_pending(&mut self) -> anyhow::Result<()> {
        if self.registered {
            self.registered = false;

            unsafe {
                self.fence.SetEventOnCompletion(self.next, self.event)?;
                WaitForSingleObject(self.event, u32::MAX);
            }
            self.next += 1;
        }

        Ok(())
    }
}

unsafe impl Send for RendererGuard {}

impl Drop for RendererGuard {
    fn drop(&mut self) {
        unsafe {
            _ = CloseHandle(self.event);
        }
    }
}
