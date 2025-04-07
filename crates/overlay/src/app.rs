use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Request, Response},
};
use scopeguard::defer;

use crate::hook::{
    dxgi,
    opengl::{self, RENDERER},
};

async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        client
            .recv(async |message| {
                match message {
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

                    Request::Direct(_) => {}

                    Request::Test => {
                        eprintln!("Test message");
                    }
                }

                Ok(Response::Success)
            })
            .await?;
    }
}

pub async fn main() -> anyhow::Result<()> {
    let client = IpcClientConn::connect().await?;

    _ = opengl::hook();
    defer!(opengl::cleanup_hook().expect("opengl hook cleanup failed"));

    _ = dxgi::hook();
    defer!(dxgi::cleanup_hook().expect("dxgi hook cleanup failed"));

    _ = run_client(client).await;

    Ok(())
}
