use anyhow::Context;
use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Request, Response},
};
use scopeguard::defer;
use tokio::select;
use tokio_util::sync::CancellationToken;

use crate::hook::opengl::{RENDERER, cleanup_hook, hook};

async fn run_client(mut client: IpcClientConn, token: CancellationToken) -> anyhow::Result<()> {
    loop {
        let recv = client.recv(async |message| {
            match message {
                Request::Close => {
                    token.cancel();
                }

                Request::Position(update_position) => {
                    if let Some(ref mut renderer) = *RENDERER.lock() {
                        renderer.position = (update_position.x, update_position.y);
                    }
                }

                Request::Bitmap(update_bitmap) => {
                    if let Some(ref mut renderer) = *RENDERER.lock() {
                        renderer.update_texture(update_bitmap.width, update_bitmap.data);
                    }
                }

                Request::Test => {
                    eprintln!("Test message");
                }
            }

            Ok(Response::Success)
        });

        select! {
            res = recv => {
                res?
            }
            _ = token.cancelled() => {
                break;
            }
        }
    }

    client.close().await?;

    Ok(())
}

pub async fn main() -> anyhow::Result<()> {
    let client = IpcClientConn::connect().await?;

    hook().context("hook failed")?;
    defer!(cleanup_hook().expect("hook cleanup failed"));

    let token = CancellationToken::new();
    select! {
        _ = token.cancelled() => {}
        _ = run_client(client, token.clone()) => {}
    };

    Ok(())
}
