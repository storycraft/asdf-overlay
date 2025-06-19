use core::ptr;

use sync_wrapper::SyncWrapper;
use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{
        Direct3D::D3D_DRIVER_TYPE_HARDWARE,
        Direct3D11::{
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
            ID3D11DeviceContext,
        },
    },
};

pub struct DxInterop {
    pub device: ID3D11Device,
    pub cx: SyncWrapper<ID3D11DeviceContext>,
}

impl DxInterop {
    pub(super) fn create() -> anyhow::Result<Self> {
        unsafe {
            let mut device = None;
            let mut cx = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )?;
            let device = device.unwrap();
            let cx = cx.unwrap();

            Ok(Self {
                device,
                cx: SyncWrapper::new(cx),
            })
        }
    }
}
