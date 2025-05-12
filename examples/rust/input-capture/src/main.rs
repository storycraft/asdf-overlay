use core::time::Duration;
use std::env::{self, current_exe};

use anyhow::{Context, bail};
use asdf_overlay_client::{
    common::{
        event::{ClientEvent, WindowEvent},
        request::SetInputCapture,
    },
    inject,
    process::OwnedProcess,
};
use tokio::{spawn, time::sleep};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let name = env::args()
        .nth(1)
        .context("processs name is not provided")?;

    // find process from first argument
    let process = OwnedProcess::find_first_by_name(name).context("process not found")?;

    // inject overlay dll into target process
    let (mut conn, mut event) = inject(
        "asdf-overlay-example".to_string(),
        process,
        Some({
            // Find built dll
            let mut current = current_exe().unwrap();
            current.pop();
            current.push("asdf_overlay.dll");

            current
        }),
        None,
    )
    .await?;

    let Some(ClientEvent::Window {
        hwnd,
        event: WindowEvent::Added,
    }) = event.recv().await
    else {
        bail!("failed to receive main window");
    };

    conn.set_input_capture(SetInputCapture {
        hwnd,
        capture: true,
    })
    .await?;
    spawn(async move {
        while let Some(event) = event.recv().await {
            dbg!(event);
        }
    });

    // sleep for 5 secs and remove overlay (dropped)
    sleep(Duration::from_secs(5)).await;

    Ok(())
}
