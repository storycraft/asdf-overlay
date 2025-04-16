use core::{
    ffi::c_void,
    hash::{Hash, Hasher},
    mem,
};

use anyhow::Context;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use rustc_hash::FxBuildHasher;
use tracing::{debug, trace};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D12::{
            D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
            D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device,
        },
    },
    core::Interface,
};

use super::HOOK;

pub type ExecuteCommandListsFn = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void);

static QUEUE_MAP: Lazy<DashMap<DeviceKey, ID3D12CommandQueue, FxBuildHasher>> =
    Lazy::new(|| DashMap::with_hasher(FxBuildHasher));

#[tracing::instrument]
pub fn get_queue_for(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    Some(QUEUE_MAP.remove(&DeviceKey::of(device))?.1)
}

#[tracing::instrument]
pub fn clear() {
    QUEUE_MAP.clear();
}

#[tracing::instrument]
pub fn cleanup() {
    QUEUE_MAP.clear();
    QUEUE_MAP.shrink_to_fit();
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_execute_command_lists(
    this: *mut c_void,
    num_command_lists: u32,
    pp_commmand_lists: *const *mut c_void,
) {
    let Some(ref execute_command_lists) = HOOK.read().execute_command_lists else {
        return;
    };
    trace!("ExecuteCommandLists called");

    unsafe {
        let queue = ID3D12CommandQueue::from_raw_borrowed(&this).unwrap();

        if queue.GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT {
            let mut device = None;
            queue.GetDevice::<ID3D12Device>(&mut device).unwrap();
            let device = device.unwrap();

            trace!(
                "found DIRECT command queue {:?} for device {:?}",
                queue, device
            );
            QUEUE_MAP.insert(DeviceKey::of(&device), queue.clone());
        }

        mem::transmute::<*const (), ExecuteCommandListsFn>(execute_command_lists.original_fn())(
            this,
            num_command_lists,
            pp_commmand_lists,
        )
    }
}

/// Get pointer to ID3D12CommandQueue::ExecuteCommandLists of D3D12_COMMAND_LIST_TYPE_DIRECT type by creating dummy device
#[tracing::instrument]
pub fn get_execute_command_lists_addr() -> anyhow::Result<ExecuteCommandListsFn> {
    unsafe {
        let mut device = None;
        D3D12CreateDevice::<_, ID3D12Device>(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
        let device = device.context("cannot create IDirect3DDevice12")?;

        let queue = device.CreateCommandQueue::<ID3D12CommandQueue>(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            ..Default::default()
        })?;
        let addr = Interface::vtable(&queue).ExecuteCommandLists;
        debug!("ID3D12CommandQueue::ExecuteCommandLists found: {:p}", addr);

        Ok(addr)
    }
}

#[derive(PartialEq, Eq)]
#[repr(transparent)]
struct DeviceKey(*const ());

impl DeviceKey {
    pub fn of(device: &ID3D12Device) -> Self {
        DeviceKey(device.as_raw() as _)
    }
}

impl Hash for DeviceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

unsafe impl Send for DeviceKey {}
unsafe impl Sync for DeviceKey {}
