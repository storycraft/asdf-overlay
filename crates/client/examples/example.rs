use core::time::Duration;
use std::env::{self, current_exe};

use anyhow::Context;
use asdf_overlay_client::{inject, process::OwnedProcess};
use asdf_overlay_common::{
    message::{Bitmap, Position, Request},
    size::PercentLength,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let name = env::args()
        .nth(1)
        .context("processs name is not provided")?;

    let mut conn = inject(
        OwnedProcess::find_first_by_name(name).context("process not found")?,
        Some({
            // Example executable is under examples directory so pop twice
            let mut current = current_exe().unwrap();
            current.pop();
            current.pop();
            current.push("asdf_overlay.dll");

            current
        }),
        None,
    )
    .await?;

    sleep(Duration::from_secs(1)).await;

    conn.request(&Request::UpdatePosition(Position {
        x: PercentLength::Length(100.0),
        y: PercentLength::Length(100.0),
    }))
    .await?;

    let mut data = Vec::new();
    for i in 0..200 {
        data.resize(i * i * 4, 0);
        rand::fill(&mut data[..]);

        conn.request(&Request::UpdateBitmap(Bitmap {
            width: i as _,
            data: data.clone(),
        }))
        .await?;
        sleep(Duration::from_millis(10)).await;
    }

    conn.request(&Request::UpdatePosition(Position {
        x: PercentLength::Length(200.0),
        y: PercentLength::Length(200.0),
    }))
    .await?;

    sleep(Duration::from_secs(1)).await;

    Ok(())
}
