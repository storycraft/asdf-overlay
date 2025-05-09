use core::{ffi::c_void, ptr};
use std::sync::Once;

use asdf_overlay_common::{
    event::{ClientEvent, WindowEvent},
    ipc::client::{IpcClientConn, IpcClientEventEmitter},
    request::{Request, SetAnchor, SetMargin, SetPosition},
    size::PercentLength,
};
use parking_lot::Mutex;
use scopeguard::defer;
use tracing::{debug, error, trace};
use windows::Win32::Foundation::HWND;

use crate::{backend::Backends, hook, util::with_dummy_hwnd};

pub struct Overlay {
    emitter: Option<IpcClientEventEmitter>,
    position: SetPosition,
    anchor: SetAnchor,
    margin: SetMargin,
}

impl Overlay {
    pub fn calc_overlay_position(&self, size: (f32, f32), screen: (u32, u32)) -> (f32, f32) {
        let screen = (screen.0 as f32, screen.1 as f32);

        let margin_left = self.margin.left.resolve(screen.0);
        let margin_top = self.margin.top.resolve(screen.1);

        let outer_width = margin_left + size.0 + self.margin.right.resolve(screen.0);
        let outer_height = margin_top + size.1 + self.margin.bottom.resolve(screen.1);

        let x =
            self.position.x.resolve(screen.0) - self.anchor.x.resolve(outer_width) + margin_left;
        let y =
            self.position.y.resolve(screen.1) - self.anchor.y.resolve(outer_height) + margin_top;

        (x, y)
    }

    pub fn emit_event(event: ClientEvent) {
        if let Some(ref mut emitter) = CURRENT.lock().emitter {
            _ = emitter.emit(event);
        }
    }

    pub fn with<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        f(&mut CURRENT.lock())
    }
}

static CURRENT: Mutex<Overlay> = Mutex::new(Overlay {
    emitter: None,
    position: SetPosition {
        x: PercentLength::ZERO,
        y: PercentLength::ZERO,
    },
    anchor: SetAnchor {
        x: PercentLength::ZERO,
        y: PercentLength::ZERO,
    },
    margin: SetMargin {
        top: PercentLength::ZERO,
        right: PercentLength::ZERO,
        bottom: PercentLength::ZERO,
        left: PercentLength::ZERO,
    },
});

#[tracing::instrument(skip(client))]
async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        let (id, req) = client.recv().await?;
        trace!("recv id: {id} req: {req:?}");

        match req {
            Request::SetPosition(position) => {
                Overlay::with(|overlay| overlay.position = position);
                client.reply(id, ())?;
            }

            Request::SetAnchor(anchor) => {
                Overlay::with(|overlay| overlay.anchor = anchor);
                client.reply(id, ())?;
            }

            Request::SetMargin(margin) => {
                Overlay::with(|overlay| overlay.margin = margin);
                client.reply(id, ())?;
            }

            Request::GetSize(get_size) => {
                client.reply(
                    id,
                    Backends::with_backend(
                        HWND(ptr::null_mut::<c_void>().with_addr(get_size.hwnd as usize)),
                        |backend| backend.size,
                    ),
                )?;
            }

            Request::SetInputCapture(cmd) => {
                let applied = Backends::with_backend(
                    HWND(ptr::null_mut::<c_void>().with_addr(cmd.hwnd as usize)),
                    |backend| {
                        backend.capture_input = cmd.capture;
                    },
                )
                .is_some();

                client.reply(id, applied)?;
            }

            Request::UpdateSharedHandle(shared) => {
                for mut backend in Backends::iter_mut() {
                    backend.pending_handle = Some(shared.clone());
                }

                client.reply(id, ())?;
            }
        }
    }
}

#[tracing::instrument]
pub async fn app(addr: &str) {
    pub async fn inner(addr: &str) -> anyhow::Result<()> {
        defer!({
            debug!("exiting");
        });

        let client = setup_ipc_client(addr).await?;
        defer!({
            debug!("cleanup start");
            hook::cleanup();
            Backends::cleanup_renderers();
            Overlay::with(|overlay| {
                overlay.emitter.take();
            });
        });

        _ = run_client(client).await;
        Ok(())
    }

    setup_once();
    if let Err(err) = inner(addr).await {
        error!("{:?}", err);
    }
}

async fn setup_ipc_client(addr: &str) -> anyhow::Result<IpcClientConn> {
    debug!("connecting ipc");
    let client = IpcClientConn::connect(addr).await?;
    debug!("ipc client connected");

    debug!("sending initial data");
    let emitter = client.create_emitter();
    // send existing windows
    for backend in Backends::iter() {
        _ = emitter.emit(ClientEvent::Window {
            hwnd: *backend.key() as _,
            event: WindowEvent::Added,
        });
    }

    Overlay::with(|overlay| {
        overlay.emitter = Some(emitter);
    });
    debug!("initial data sent");

    Ok(client)
}

fn setup_once() {
    #[cfg(debug_assertions)]
    fn setup_tracing() {
        use tracing::level_filters::LevelFilter;

        use crate::dbg::WinDbgMakeWriter;

        tracing_subscriber::fmt::fmt()
            .with_ansi(false)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::TRACE)
            .with_writer(WinDbgMakeWriter::new())
            .init();
    }

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(debug_assertions)]
        setup_tracing();

        with_dummy_hwnd(|dummy_hwnd| {
            hook::install(dummy_hwnd).expect("hook initialization failed");
            debug!("hook installed");
        })
        .expect("failed to create dummy window");
    });
}
