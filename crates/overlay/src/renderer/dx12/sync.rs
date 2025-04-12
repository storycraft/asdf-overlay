use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::Direct3D12::*,
    System::Threading::{CreateEventA, WaitForSingleObject},
};

use super::MAX_RENDER_TARGETS;

pub struct RendererFence {
    fence: ID3D12Fence,
    event: HANDLE,
    fence_val: [u64; MAX_RENDER_TARGETS],
    back_buffer_index: usize,
    last_queue: Option<ID3D12CommandQueue>,
}

impl RendererFence {
    pub fn new(device: &ID3D12Device) -> anyhow::Result<Self> {
        Ok(Self {
            fence: unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? },
            event: unsafe { CreateEventA(None, false, false, None)? },
            fence_val: [1; MAX_RENDER_TARGETS],
            back_buffer_index: MAX_RENDER_TARGETS - 1,
            last_queue: None,
        })
    }

    pub fn wait_gpu(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        let val = self.fence_val[self.back_buffer_index];
        unsafe {
            queue.Signal(&self.fence, val)?;
            self.fence.SetEventOnCompletion(val, self.event)?;
            WaitForSingleObject(self.event, u32::MAX);
        }

        self.fence_val[self.back_buffer_index] += 1;
        Ok(())
    }

    pub fn register(&mut self, queue: &ID3D12CommandQueue) -> anyhow::Result<()> {
        if let Some(ref last_queue) = self.last_queue {
            if queue == last_queue {
                return Ok(());
            }
        }

        self.last_queue = Some(queue.clone());
        Ok(())
    }

    pub fn wait_pending(&mut self) -> anyhow::Result<()> {
        if let Some(last_queue) = self.last_queue.take() {
            let val = self.fence_val[self.back_buffer_index];

            unsafe {
                last_queue.Signal(&self.fence, val)?;
                self.fence.SetEventOnCompletion(val, self.event)?;
                WaitForSingleObject(self.event, u32::MAX);
            }
            self.fence_val[self.back_buffer_index] += 1;
        }

        Ok(())
    }

    pub fn sync_next_frame(&mut self, next_back_buffer_index: usize) -> anyhow::Result<()> {
        let current_frame_val = self.fence_val[self.back_buffer_index];
        self.back_buffer_index = next_back_buffer_index;

        if let Some(last_queue) = self.last_queue.as_ref() {
            let old_next_frame_val = self.fence_val[self.back_buffer_index];
            unsafe {
                last_queue.Signal(&self.fence, current_frame_val)?;
                if self.fence.GetCompletedValue() < old_next_frame_val {
                    self.fence
                        .SetEventOnCompletion(old_next_frame_val, self.event)?;
                    WaitForSingleObject(self.event, u32::MAX);
                }
            }
        }

        self.fence_val[self.back_buffer_index] = current_frame_val + 1;
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
