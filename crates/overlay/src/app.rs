use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Request, Response},
};
use scopeguard::defer;
use windows::Win32::System::Console::{AllocConsole, FreeConsole};

use crate::hook::{dxgi, opengl};

async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        client
            .recv(async |message| {
                match message {
                    Request::Position(update_position) => {
                        if let Some(ref mut renderer) = *opengl::RENDERER.lock() {
                            renderer.position = (update_position.x, update_position.y);
                        }

                        if let Some(ref mut renderer) = *dxgi::RENDERER.dx11.lock() {
                            renderer.position = (update_position.x, update_position.y);
                        }
                    }

                    Request::Bitmap(update_bitmap) => {
                        if let Some(ref mut renderer) = *opengl::RENDERER.lock() {
                            renderer.update_texture(update_bitmap.width, update_bitmap.data);
                        } else if let Some(ref mut renderer) = *dxgi::RENDERER.dx11.lock() {
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
    
    unsafe {
        AllocConsole()?;
    }

    _ = opengl::hook();
    defer!(opengl::cleanup_hook().expect("opengl hook cleanup failed"));

    _ = dxgi::hook();
    defer!(dxgi::cleanup_hook().expect("dxgi hook cleanup failed"));

    _ = run_client(client).await;
    
    unsafe {
        FreeConsole()?;
    }

    Ok(())
}
