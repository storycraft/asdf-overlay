use windows::{
    Win32::Graphics::Direct3D12::{ID3D12CommandList, ID3D12CommandQueue},
    core::Interface,
};

use super::HOOK;

#[tracing::instrument]
pub unsafe fn call_original_execute_command_lists(
    queue: &ID3D12CommandQueue,
    command_lists: &[Option<ID3D12CommandList>],
) {
    match HOOK.execute_command_lists.get() {
        Some(hook) => unsafe {
            hook.original_fn()(
                queue.as_raw(),
                command_lists.len() as _,
                command_lists.as_ptr().cast(),
            )
        },
        None => unsafe { queue.ExecuteCommandLists(command_lists) },
    }
}
