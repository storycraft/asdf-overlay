use std::sync::Once;

use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Anchor, Margin, Position, Request, Response},
    size::PercentLength,
};
use parking_lot::RwLock;
use scopeguard::defer;
use tracing::{debug, error, trace};

use crate::{hook, renderer::Renderers, util::with_dummy_hwnd};

pub struct Overlay {
    position: Position,
    anchor: Anchor,
    margin: Margin,
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

    pub fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        f(&CURRENT.read())
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        f(&mut CURRENT.write())
    }
}

static CURRENT: RwLock<Overlay> = RwLock::new(Overlay {
    position: Position {
        x: PercentLength::ZERO,
        y: PercentLength::ZERO,
    },
    anchor: Anchor {
        x: PercentLength::ZERO,
        y: PercentLength::ZERO,
    },
    margin: Margin {
        top: PercentLength::ZERO,
        right: PercentLength::ZERO,
        bottom: PercentLength::ZERO,
        left: PercentLength::ZERO,
    },
});

#[tracing::instrument(skip(client))]
async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        client
            .recv(async |message| {
                trace!("recv: {:?}", message);

                match message {
                    Request::UpdatePosition(position) => {
                        Overlay::with_mut(|overlay| overlay.position = position);
                    }

                    Request::UpdateAnchor(anchor) => {
                        Overlay::with_mut(|overlay| overlay.anchor = anchor);
                    }

                    Request::UpdateMargin(margin) => {
                        Overlay::with_mut(|overlay| overlay.margin = margin);
                    }

                    Request::UpdateBitmap(bitmap) => {
                        Renderers::with(|renderer| {
                            renderer.update_texture(bitmap);
                        });
                    }

                    Request::UpdateShtex(shared) => {
                        trace!(shared.handle);
                    }

                    _ => {}
                }

                Ok(Response::Success)
            })
            .await?;
    }
}

#[tracing::instrument]
pub async fn app(addr: &str) {
    pub async fn inner(addr: &str) -> anyhow::Result<()> {
        defer!({
            debug!("exiting");
        });

        debug!("connecting ipc");
        let client = IpcClientConn::connect(addr).await?;
        debug!("ipc client connected");
        defer!({
            debug!("cleanup start");
            Renderers::with(|renderers| {
                renderers.cleanup();
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
