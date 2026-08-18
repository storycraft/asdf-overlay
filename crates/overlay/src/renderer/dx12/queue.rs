#![allow(non_snake_case)]

use core::ffi::c_void;

use windows::Win32::Graphics::Direct3D12::ID3D12Object;
use windows_core::{HRESULT, IUnknown, IUnknown_Vtbl, Ref, interface};

/// Compatibility interface for ID3D12CommandQueue to support AcquireKeyedMutex and ReleaseKeyedMutex.
///
/// Undocumented, but can be found in D3D12TranslationLayer (d3d12compatibility.h).
/// https://github.com/microsoft/D3D12TranslationLayer/blob/7881738b5da915e4312cf7ddeff2cb2edfbaf536/external/d3d12compatibility.h#L237
#[interface("7974c836-9520-4cda-8d43-d996622e8926")]
pub unsafe trait ID3D12CompatibilityQueue: IUnknown {
    pub fn AcquireKeyedMutex(
        &self,
        object: Ref<ID3D12Object>,
        key: u64,
        dw_timeout: u32,
        p_reserved: *mut c_void,
        reserved: u32,
    ) -> HRESULT;

    pub fn ReleaseKeyedMutex(
        &self,
        object: Ref<ID3D12Object>,
        key: u64,
        p_reserved: *mut c_void,
        reserved: u32,
    ) -> HRESULT;
}
