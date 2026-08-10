use core::time::Duration;
use std::env;

use anyhow::{Context, bail};
use asdf_overlay_client::{
    OverlayDll,
    common::{
        event::{OverlayEvent, surface::SurfaceEvent},
        request::surface::SetPosition,
    },
    inject,
    surface::OverlaySurface,
};
use tokio::time::sleep;

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

    let Some(OverlayEvent::Surface {
        id,
        event: SurfaceEvent::Added { .. },
    }) = event.recv().await
    else {
        bail!("failed to receive main surface");
    };

    sleep(Duration::from_secs(1)).await;

    // set initial position
    conn.surface(id)
        .request(SetPosition { x: 100, y: 100 })
        .await?;

    let mut surface: OverlaySurface = OverlaySurface::new(None)?;
    let mut data = Vec::new();
    for i in 0..200 {
        // make noise rectangle bigger
        data.resize(i * i * 4, 0);
        rand::fill(&mut data[..]);

        let update = surface.update_bitmap(i as _, &data)?;
        if let Some(shared) = update {
            conn.surface(id).request(shared).await?;
        }

        sleep(Duration::from_millis(10)).await;
    }

    // move rectangle
    conn.surface(id)
        .request(SetPosition { x: 200, y: 200 })
        .await?;

    // sleep for 1 secs and remove overlay (dropped)
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
