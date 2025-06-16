use core::time::Duration;
use std::env;

use anyhow::Context;
use asdf_overlay_client::{
    OverlayDll,
    common::{request::SetPosition, size::PercentLength},
    inject,
    surface::OverlaySurface,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pid = env::args().nth(1).context("processs pid is not provided")?;

    let dll_dir = env::current_dir().expect("cannot find pwd");

    // inject overlay dll into target process
    let (mut conn, _) = inject(
        pid.parse::<u32>().context("invalid pid")?,
        OverlayDll {
            x64: Some(&dll_dir.join("asdf_overlay-x64.dll")),
            x86: Some(&dll_dir.join("asdf_overlay-x86.dll")),
            arm64: Some(&dll_dir.join("asdf_overlay-aarch64.dll")),
        },
        None,
    )
    .await?;

    sleep(Duration::from_secs(1)).await;

    // set initial position
    conn.set_position(SetPosition {
        x: PercentLength::Length(100.0),
        y: PercentLength::Length(100.0),
    })
    .await?;

    let mut surface: OverlaySurface = OverlaySurface::new()?;
    let mut data = Vec::new();
    for i in 0..200 {
        // make noise rectangle bigger
        data.resize(i * i * 4, 0);
        rand::fill(&mut data[..]);

        let update = surface.update_bitmap(i as _, &data)?;
        if let Some(shared) = update {
            conn.update_shtex(shared).await?;
        }

        sleep(Duration::from_millis(10)).await;
    }

    // move rectangle
    conn.set_position(SetPosition {
        x: PercentLength::Length(200.0),
        y: PercentLength::Length(200.0),
    })
    .await?;

    // sleep for 1 secs and remove overlay (dropped)
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
