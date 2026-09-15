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
                D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIAdapter, IDXGIDevice,
            },
        },
    },
    core::Interface,
};

/// A D3D11 device for sharing overlay textures with graphics backends.
#[non_exhaustive]
pub struct DxInterop {
    /// The GPU adapter on which overlay textures must be created.
    pub gpu_id: GpuLuid,

    /// Whether this process can share textures with a keyed mutex.
    pub keyed_mutex: bool,

    /// Interop Direct3D 11 device.
    pub device: ID3D11Device,

    /// Interop Direct3D 11 device context.
    pub cx: Mutex<ID3D11DeviceContext>,
}

impl DxInterop {
    /// Create a BGRA-capable D3D11 device on the supplied adapter or the default
    /// hardware GPU.
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
                keyed_mutex: supports_keyed_mutex(&device),
                device,
                cx: Mutex::new(cx),
            })
        }
    }
}

/// Whether `device` can share textures with a keyed mutex.
///
/// Some processes reject `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`, and cannot open a
/// texture created with it either, while a plain shared texture still works.
fn supports_keyed_mutex(device: &ID3D11Device) -> bool {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 1,
        Height: 1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as _,
        CPUAccessFlags: 0,
        MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
    };

    let mut texture = None::<ID3D11Texture2D>;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }.is_ok()
}
