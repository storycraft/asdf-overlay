use core::{mem, ptr};
use std::{
    env,
    sync::{Arc, Mutex},
};

use anyhow::{Context, bail};
use asdf_overlay_client::{
    OverlayDll,
    client::{IpcClientConn, IpcClientEventStream},
    common::{
        event::{OverlayEvent, surface::SurfaceEvent},
        request::surface::UpdateSharedHandle,
    },
    surface::OverlaySurface,
};
use scopeguard::defer;
use tao::{
    event_loop::{ControlFlow, DeviceEventFilter, EventLoop},
    window::WindowBuilder,
};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    UI::Composition::Compositor,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Texture2D,
            },
            Dxgi::IDXGIDevice,
        },
        System::WinRT::{
            CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            DispatcherQueueOptions,
        },
    },
    core::{IUnknown, Interface, Ref},
};
use windows_numerics::Vector2;
use wry::{
    WebViewBuilder, WebViewBuilderExtWindows,
    dpi::{PhysicalSize, Position, Size},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pid = env::args()
        .nth(1)
        .context("processs pid is not provided")?
        .parse::<u32>()
        .context("pid is not a valid number")?;

    let (conn, mut event) = setup_overlay_client(pid).await?;
    let conn = Arc::new(tokio::sync::Mutex::new(conn));

    let surface_id = fetch_main_surface_id(&mut event).await?;
    eprintln!("main surface id: {surface_id}");

    let (d3d11_device, d3d11_cx) = unsafe {
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

        (device.unwrap(), cx.unwrap())
    };

    // Setup overlay surface
    let overlay_surface = Arc::new(Mutex::new(OverlaySurface::<2>::new_with_device(
        d3d11_device.clone(),
        d3d11_cx,
    )));

    let view_size = (1280, 720);

    // Setup winrt dispatcher
    let _dispatcher_controller = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: mem::size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })
    };

    // Setup Compositor
    let compositor = Compositor::new()?;
    let visual = compositor.CreateContainerVisual()?;
    visual.SetSize(Vector2 {
        X: view_size.0 as f32,
        Y: view_size.1 as f32,
    })?;

    // Setup GraphicsCaptureItem and Direct3D11CaptureFramePool
    let capture_item = GraphicsCaptureItem::CreateFromVisual(&visual)?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &unsafe { CreateDirect3D11DeviceFromDXGIDevice(&d3d11_device.cast::<IDXGIDevice>()?)? }
            .cast::<IDirect3DDevice>()?,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        capture_item.Size()?,
    )?;

    // Setup channel for frame updates
    let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<UpdateSharedHandle>();

    // Handle frames
    let handler_overlay_surface = overlay_surface.clone();
    pool.FrameArrived(&TypedEventHandler::new(
        move |sender: Ref<Direct3D11CaptureFramePool>, _| {
            let pool: &Direct3D11CaptureFramePool = sender.ok()?;
            let next_frame = pool.TryGetNextFrame()?;
            defer!({
                _ = next_frame.Close();
            });

            let surface = next_frame.Surface()?;
            let desc = surface.Description()?;
            let interop = surface.cast::<IDirect3DDxgiInterfaceAccess>()?;
            let tex = unsafe { interop.GetInterface::<ID3D11Texture2D>()? };

            if let Some(update) = handler_overlay_surface
                .lock()
                .unwrap()
                .update_from_texture(desc.Width as _, desc.Height as _, &tex, None)
                .unwrap()
            {
                _ = update_tx.send(update);
            }

            Ok(())
        },
    ))?;

    // Setup frame update task
    tokio::spawn(async move {
        while let Some(update) = update_rx.recv().await {
            conn.lock()
                .await
                .surface(surface_id)
                .request(update)
                .await?;
        }

        Ok::<_, anyhow::Error>(())
    });

    // Create capture session
    let capture_session = pool.CreateCaptureSession(&capture_item)?;
    capture_session.StartCapture()?;

    let event_loop = EventLoop::new();
    event_loop.set_device_event_filter(DeviceEventFilter::Never);

    // Setup invisible window for webview
    let window = WindowBuilder::new()
        .with_inner_size(tao::dpi::LogicalSize::new(0.0, 0.0))
        .with_visible(false)
        .build(&event_loop)?;

    // Setup composition webview
    let webview = WebViewBuilder::new()
        .with_url("https://v2.tauri.app/")
        .with_transparent(true)
        .with_composition_visual_target(unsafe {
            mem::transmute::<IUnknown, _>(visual.cast::<IUnknown>()?)
        })
        .build(&window)?;

    webview.open_devtools();

    // Set webview size
    webview.set_bounds(wry::Rect {
        position: Position::Physical(Default::default()),
        size: Size::Physical(PhysicalSize::new(view_size.0, view_size.1)),
    })?;

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
    });
}

async fn fetch_main_surface_id(event: &mut IpcClientEventStream) -> anyhow::Result<u64> {
    loop {
        let Some(event) = event.recv().await else {
            bail!("failed to receive main surface");
        };

        if let OverlayEvent::Surface {
            id,
            event: SurfaceEvent::Added { .. },
        } = event
        {
            return Ok(id);
        }
    }
}

async fn setup_overlay_client(pid: u32) -> anyhow::Result<(IpcClientConn, IpcClientEventStream)> {
    let dll_dir = env::current_dir()
        .expect("cannot find pwd")
        .join("packages/core");

    // inject overlay dll into target process
    let (conn, event) = asdf_overlay_client::inject(
        pid,
        OverlayDll {
            x64: Some(&dll_dir.join("asdf_overlay-x64.dll")),
            x86: Some(&dll_dir.join("asdf_overlay-x86.dll")),
            arm64: Some(&dll_dir.join("asdf_overlay-aarch64.dll")),
        },
        None,
    )
    .await?;

    Ok((conn, event))
}
