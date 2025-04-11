use anyhow::Context;
use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Anchor, Margin, Position, Request, Response},
};
use parking_lot::RwLock;
use scopeguard::defer;

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
        f(CURRENT.read().as_ref().expect("Overlay is not initialized"))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        f(&mut *CURRENT
            .write()
            .as_mut()
            .expect("Overlay is not initialized"))
    }
}

static CURRENT: RwLock<Option<Overlay>> = RwLock::new(None);

async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        client
            .recv(async |message| {
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
                        Renderers::get().update_texture(bitmap);
                    }

                    Request::Direct(_) => {}
                }

                Ok(Response::Success)
            })
            .await?;
    }
}

pub async fn main() -> anyhow::Result<()> {
    *CURRENT.write() = Some(Overlay {
        position: Position::default(),
        anchor: Anchor::default(),
        margin: Margin::default(),
    });

    let client = IpcClientConn::connect().await?;

    defer!({
        hook::cleanup();
        Renderers::get().cleanup();
    });

    with_dummy_hwnd(|dummy_hwnd| {
        hook::install(dummy_hwnd).context("hook initialization failed")?;
        Ok::<_, anyhow::Error>(())
    })
    .context("failed to create dummy window")??;

    _ = run_client(client).await;

    Ok(())
}
