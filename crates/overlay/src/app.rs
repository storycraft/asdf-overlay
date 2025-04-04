use core::pin::pin;

use anyhow::Context;
use asdf_overlay_common::{
    ipc::server::{IpcServerConn, listen},
    message::{Request, Response},
};
use futures::StreamExt;
use tokio::{io::AsyncRead, select};
use tokio_util::sync::CancellationToken;

use crate::hook::opengl::{RENDERER, cleanup_hook, hook};

async fn run_server(
    mut conn: IpcServerConn<impl AsyncRead + Unpin>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        let recv = conn.recv(async |message| {
            match message {
                Request::Close => {
                    token.cancel();
                }

                Request::Position(update_position) => {
                    if let Some(ref mut renderer) = *RENDERER.lock() {
                        renderer.position = (update_position.x, update_position.y);
                    }
                }

                Request::Texture(update_texture) => {
                    if let Some(ref mut renderer) = *RENDERER.lock() {
                        renderer.update_texture(update_texture.width, update_texture.data);
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

    conn.close().await?;

    Ok(())
}

pub async fn main() -> anyhow::Result<()> {
    hook().context("hook failed")?;

    let token = CancellationToken::new();

    let mut tasks = Vec::new();
    let server = async {
        let conn = listen()?;
        let mut conn = pin!(conn);
        while let Some(conn) = conn.next().await.transpose()? {
            tasks.push(tokio::spawn(run_server(conn, token.clone())));
        }

        Ok::<_, anyhow::Error>(())
    };

    select! {
        _ = token.cancelled() => {}

        res = server => {
            res?
        }
    };

    cleanup_hook().context("hook cleanup failed. program may crash")?;

    for task in tasks {
        task.await??;
    }

    Ok(())
}
