use core::ptr;

use anyhow::Context;
use asdf_overlay_event::GpuLuid;
use sync_wrapper::SyncWrapper;
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

pub struct DxInterop {
    gpu_id: GpuLuid,
    pub device: ID3D11Device,
    pub cx: SyncWrapper<ID3D11DeviceContext>,
}

impl DxInterop {
    pub(super) fn create(adapter: Option<&IDXGIAdapter>) -> anyhow::Result<Self> {
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
            .context("D3D11CreateDevice failed")?;
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
                cx: SyncWrapper::new(cx),
            })
        }
    }

    pub const fn gpu_id(&self) -> GpuLuid {
        self.gpu_id
    }
}
