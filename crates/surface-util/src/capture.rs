//! Screen capture utility for overlay surface using Windows.Graphics.Capture.

use core::ptr;
use std::sync::Mutex;

use asdf_overlay_common::request::surface::UpdateSharedHandle;
use scopeguard::defer;
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN},
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            },
            Dxgi::{IDXGIAdapter, IDXGIDevice},
        },
        System::WinRT::Direct3D11::{
            CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
        },
    },
    core::{Interface, Ref},
};

use crate::surface::OverlaySurface;

/// Captures an item into a shared overlay texture.
///
/// Call [`Self::start`] to begin capture. The callback runs on a capture worker when
/// the shared handle changes; subsequent frames update the same texture. Callback
/// errors are returned to the event handler; texture-update failures panic there.
///
/// The pool retains the initial capture size, so resizing the item may crop frames.
/// Shared textures must use the consumer's GPU adapter.
pub struct D3DCapturePool {
    pool: Direct3D11CaptureFramePool,
    item: GraphicsCaptureItem,
    session: Option<GraphicsCaptureSession>,
}

impl D3DCapturePool {
    /// Create a stopped capture pool on the supplied adapter or the default hardware GPU.
    pub fn new<F>(
        adapter: Option<&IDXGIAdapter>,
        item: GraphicsCaptureItem,
        on_capture: F,
    ) -> windows::core::Result<Self>
    where
        F: Fn(UpdateSharedHandle) -> windows::core::Result<()> + 'static + Send,
    {
        let mut device = None;
        let mut cx = None;
        unsafe {
            D3D11CreateDevice(
                adapter,
                if adapter.is_none() {
                    D3D_DRIVER_TYPE_HARDWARE
                } else {
                    D3D_DRIVER_TYPE_UNKNOWN
                },
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )?;
        }
        let device = device.unwrap();
        let cx = cx.unwrap();

        Self::new_with_device(device, cx, item, on_capture)
    }

    /// Create a stopped capture pool using a BGRA-capable device and its immediate context.
    ///
    /// Synchronize external use of the context with capture.
    pub fn new_with_device<F>(
        device: ID3D11Device,
        cx: ID3D11DeviceContext,
        item: GraphicsCaptureItem,
        on_capture: F,
    ) -> windows::core::Result<Self>
    where
        F: Fn(UpdateSharedHandle) -> windows::core::Result<()> + 'static + Send,
    {
        let rt_device =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)? }
                .cast::<IDirect3DDevice>()?;

        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &rt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            item.Size()?,
        )?;

        let surface = Mutex::new(OverlaySurface::<2>::new_with_device(device, cx));
        pool.FrameArrived(&TypedEventHandler::new(
            move |sender: Ref<Direct3D11CaptureFramePool>, _| {
                let Some(update) = Self::frame_handler(sender.ok()?, &mut surface.lock().unwrap())?
                else {
                    return Ok(());
                };

                on_capture(update)
            },
        ))?;

        Ok(Self {
            pool,
            item,
            session: None,
        })
    }

    fn frame_handler(
        pool: &Direct3D11CaptureFramePool,
        overlay_surface: &mut OverlaySurface,
    ) -> windows::core::Result<Option<UpdateSharedHandle>> {
        let next_frame = pool.TryGetNextFrame()?;
        defer!({
            _ = next_frame.Close();
        });

        let surface = next_frame.Surface()?;
        let desc = surface.Description()?;
        let interop = surface.cast::<IDirect3DDxgiInterfaceAccess>()?;
        let tex = unsafe { interop.GetInterface::<ID3D11Texture2D>()? };

        Ok(overlay_surface
            .update_from_texture(desc.Width as _, desc.Height as _, &tex, None)
            .unwrap())
    }

    /// Start capture. Call only while stopped.
    pub fn start(&mut self) -> windows::core::Result<()> {
        let session = self.pool.CreateCaptureSession(&self.item)?;
        session.StartCapture()?;
        self.session = Some(session);

        Ok(())
    }

    /// Stop capture, doing nothing if already stopped.
    ///
    /// The consumer retains its last texture until sent a removal update.
    pub fn stop(&mut self) -> windows::core::Result<()> {
        if let Some(session) = self.session.take() {
            session.Close()?;
        }

        Ok(())
    }
}

impl Drop for D3DCapturePool {
    fn drop(&mut self) {
        _ = self.stop();
        _ = self.pool.Close();
    }
}
