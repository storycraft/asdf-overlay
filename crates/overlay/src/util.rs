//! Common utilies used in many modules internally.

use core::mem::{self, ManuallyDrop};

use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LUID, RECT},
        Graphics::Dxgi::{IDXGIAdapter, IDXGIFactory, IDXGIKeyedMutex},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                CreateDialogIndirectParamA, DLGITEMTEMPLATE, DLGTEMPLATE, DestroyWindow,
                GetClientRect,
            },
        },
    },
    core::Interface,
};

// Cloning COM objects for ManuallyDrop<Option<T>> never decrease ref count and leak wtf
// as per: https://github.com/microsoft/windows-rs/blob/83d4e0b4d49d004f52523614f292bc1526142052/crates/samples/windows/direct3d12/src/main.rs#L493
pub unsafe fn wrap_com_manually_drop<T: Interface>(inf: &T) -> ManuallyDrop<Option<T>> {
    unsafe { mem::transmute_copy(inf) }
}

/// Get DPI aware client area size of the window.
pub fn get_client_size(hwnd: HWND) -> anyhow::Result<(u32, u32)> {
    unsafe {
        let old_context = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
        defer!({
            SetThreadDpiAwarenessContext(old_context);
        });

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)?;
        Ok((rect.right as u32, rect.bottom as u32))
    }
}

/// Create dummy class and window for various operation.
///
/// Creating another dummy windows in closures fail.
pub fn with_dummy_hwnd<R>(f: impl FnOnce(HWND) -> R) -> anyhow::Result<R> {
    let template = (DLGTEMPLATE::default(), [DLGITEMTEMPLATE::default(); 3]);
    unsafe {
        let hwnd = CreateDialogIndirectParamA(
            None,
            (&raw const template).cast::<DLGTEMPLATE>(),
            None,
            None,
            LPARAM(0),
        )?;
        defer!({
            _ = DestroyWindow(hwnd);
        });

        Ok(f(hwnd))
    }
}

/// If [`IDXGIKeyedMutex`],
/// * Exists, acquire the mutext with `0` value key, run closure and release.
/// * Not exists, just run closure.
#[inline]
pub fn with_keyed_mutex<R>(
    mutex: Option<&IDXGIKeyedMutex>,
    f: impl FnOnce() -> R,
) -> windows::core::Result<R> {
    match mutex {
        Some(mutex) => {
            unsafe { mutex.AcquireSync(0, u32::MAX)? };
            defer!(unsafe {
                _ = mutex.ReleaseSync(0);
            });

            Ok(f())
        }
        None => Ok(f()),
    }
}

pub fn find_adapter_by_luid(factory: &IDXGIFactory, luid: LUID) -> Option<IDXGIAdapter> {
    let mut i = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters(i) } {
        i += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc() }) else {
            continue;
        };

        if desc.AdapterLuid == luid {
            return Some(adapter);
        }
    }

    None
}
