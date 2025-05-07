use core::time::Duration;
use std::env::{self, current_exe};

use anyhow::Context;
use asdf_overlay_client::{
    common::{
        message::{Position, Request},
        size::PercentLength,
    },
    inject,
    process::OwnedProcess,
    surface::OverlaySurface,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let name = env::args()
        .nth(1)
        .context("processs name is not provided")?;

    // find process from first argument
    let process = OwnedProcess::find_first_by_name(name).context("process not found")?;

    // inject overlay dll into target process
    let (mut conn, _) = inject(
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

    sleep(Duration::from_secs(1)).await;

    // set initial position
    conn.request(Request::UpdatePosition(Position {
        x: PercentLength::Length(100.0),
        y: PercentLength::Length(100.0),
    }))
    .await?;

    let mut surface: OverlaySurface = OverlaySurface::new()?;
    let mut data = Vec::new();
    for i in 0..200 {
        // make noise rectangle bigger
        data.resize(i * i * 4, 0);
        rand::fill(&mut data[..]);

        let update = surface.update_bitmap(i as _, &data)?;
        if let Some(shared) = update {
            conn.request(Request::UpdateShtex(shared)).await?;
        }

        sleep(Duration::from_millis(10)).await;
    }

    // move rectangle
    conn.request(Request::UpdatePosition(Position {
        x: PercentLength::Length(200.0),
        y: PercentLength::Length(200.0),
    }))
    .await?;

    // sleep for 1 secs and remove overlay (dropped)
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
