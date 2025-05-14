use std::env::{self, current_exe};

use anyhow::{Context, bail};
use asdf_overlay_client::{
    common::{
        event::{ClientEvent, WindowEvent},
        key::Key,
        request::SetInputCaptureKeybind,
    },
    inject,
    process::OwnedProcess,
};

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

    conn.set_input_capture_keybind(SetInputCaptureKeybind {
        hwnd,
        // Left Shift = 0x10 (without extended flag), A = 0x41
        keybind: [Key::new(0x10, false), Key::new(0x41, false), None, None],
    })
    .await?;

    while let Some(event) = event.recv().await {
        dbg!(&event);

        if let ClientEvent::Window {
            event: WindowEvent::InputCaptureEnd,
            ..
        } = event
        {
            break;
        }
    }

    Ok(())
}
