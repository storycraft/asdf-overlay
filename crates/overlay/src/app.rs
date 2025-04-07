use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Request, Response},
};
use scopeguard::defer;
use tokio::select;

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

    let opengl_hooked = opengl::hook().is_ok();
    defer!(opengl::cleanup_hook().expect("opengl hook cleanup failed"));

    let dxgi_hooked = dxgi::hook().is_ok();
    defer!(dxgi::cleanup_hook().expect("dxgi hook cleanup failed"));

    if !opengl_hooked || !dxgi_hooked {
        return Ok(());
    }

    select! {
        _ = run_client(client) => {}
    };

    Ok(())
}
