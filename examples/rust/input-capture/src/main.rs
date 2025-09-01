use std::env;

use anyhow::{Context, bail};
use asdf_overlay_client::{
    OverlayDll,
    common::request::BlockInput,
    event::{OverlayEvent, WindowEvent},
    inject,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pid = env::args().nth(1).context("processs pid is not provided")?;

    let dll_dir = env::current_dir().expect("cannot find pwd");

    // inject overlay dll into target process
    let (mut conn, mut event) = inject(
        pid.parse::<u32>().context("invalid pid")?,
        OverlayDll {
            x64: Some(&dll_dir.join("asdf_overlay-x64.dll")),
            x86: Some(&dll_dir.join("asdf_overlay-x86.dll")),
            arm64: Some(&dll_dir.join("asdf_overlay-aarch64.dll")),
        },
        None,
    )
    .await?;

    let Some(OverlayEvent::Window {
        id,
        event: WindowEvent::Added { .. },
    }) = event.recv().await
    else {
        bail!("failed to receive main window");
    };

    conn.window(id).request(BlockInput { block: true }).await?;

    while let Some(event) = event.recv().await {
        dbg!(&event);

        if let OverlayEvent::Window {
            event: WindowEvent::InputBlockingEnded,
            ..
        } = event
        {
            break;
        }
    }

    Ok(())
}
