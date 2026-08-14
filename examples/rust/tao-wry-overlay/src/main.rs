use core::mem;
use std::{env, sync::Arc};

use anyhow::{Context, bail};
use asdf_overlay_client::{
    OverlayDll,
    client::{IpcClientConn, IpcClientEventStream},
    common::{
        event::{OverlayEvent, surface::SurfaceEvent},
        request::surface::UpdateSharedHandle,
    },
};
use asdf_overlay_surface_util::capture::D3DCapturePool;
use tao::{
    event_loop::{ControlFlow, DeviceEventFilter, EventLoop},
    window::WindowBuilder,
};
use windows::{
    Graphics::Capture::GraphicsCaptureItem,
    UI::Composition::Compositor,
    Win32::System::WinRT::{
        CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
        DispatcherQueueOptions,
    },
    core::{IUnknown, Interface},
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

    // Setup GraphicsCaptureItem
    let capture_item = GraphicsCaptureItem::CreateFromVisual(&visual)?;

    // Setup channel for frame updates
    let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<UpdateSharedHandle>();

    // Handle frames

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

    let mut capture_pool = D3DCapturePool::new(None, capture_item, move |update| {
        _ = update_tx.send(update);

        Ok(())
    })?;
    capture_pool.start()?;

    let event_loop = EventLoop::new();
    event_loop.set_device_event_filter(DeviceEventFilter::Never);

    // Setup invisible window for webview
    let window = WindowBuilder::new()
        .with_inner_size(tao::dpi::LogicalSize::new(0.0, 0.0))
        .with_always_on_top(true)
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
