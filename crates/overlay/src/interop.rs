use core::ptr;

use anyhow::Context;
use asdf_overlay_event::GpuLuid;
use parking_lot::Mutex;
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN},
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext,
            },
            Dxgi::{IDXGIAdapter, IDXGIDevice},
        },
    },
    core::Interface,
};

/// Direct3D 11 device for storing and sharing overlay texture with other graphics backend.
#[non_exhaustive]
pub struct DxInterop {
    /// This is the GPU adapter used by the surface.
    /// Overlay surface texture must be created with this GPU.
    /// Otherwise, surface cannot be rendered.
    pub gpu_id: GpuLuid,

    /// Interop Direct3D 11 device.
    pub device: ID3D11Device,

    /// Interop Direct3D 11 device context.
    pub cx: Mutex<ID3D11DeviceContext>,
}

impl DxInterop {
    /// Create new [`DxInterop`].
    /// * If `adapter` is provided, it will use provided GPU adapter.
    /// * If `adapter` it not provided, it will use system provided GPU adapter.
    pub fn new(adapter: Option<&IDXGIAdapter>) -> anyhow::Result<Self> {
        unsafe {
            let mut device = None;
            let mut cx = None;
            D3D11CreateDevice(
                adapter,
                if adapter.is_some() {
                    D3D_DRIVER_TYPE_UNKNOWN
                } else {
                    D3D_DRIVER_TYPE_HARDWARE
                },
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )
            .context("failed to create D3D11 interop device")?;
            let device = device.unwrap();
            let cx = cx.unwrap();

            let luid = device
                .cast::<IDXGIDevice>()?
                .GetAdapter()?
                .GetDesc()?
                .AdapterLuid;
            Ok(Self {
                gpu_id: GpuLuid {
                    low: luid.LowPart,
                    high: luid.HighPart,
                },
                device,
                cx: Mutex::new(cx),
            })
        }
    }
}
