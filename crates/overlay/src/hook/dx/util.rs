use core::mem;

use windows::{
    Win32::Graphics::Direct3D12::{ID3D12CommandList, ID3D12CommandQueue},
    core::Interface,
};

use super::{HOOK, dx12::ExecuteCommandListsFn};

#[tracing::instrument]
pub unsafe fn call_original_execute_command_lists(
    queue: &ID3D12CommandQueue,
    command_lists: &[Option<ID3D12CommandList>],
) {
    match HOOK.read().execute_command_lists {
        Some(ref hook) => unsafe {
            mem::transmute::<*const (), ExecuteCommandListsFn>(hook.original_fn())(
                queue.as_raw(),
                command_lists.len() as _,
                command_lists.as_ptr().cast(),
            )
        },
        None => unsafe { queue.ExecuteCommandLists(command_lists) },
    }
}
