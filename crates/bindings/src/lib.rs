pub mod event;
#[cfg(feature = "overlay")]
pub mod overlay;
#[cfg(feature = "surface-util")]
pub mod surface_util;
pub mod types;
#[cfg(feature = "window")]
pub mod window;

use std::sync::LazyLock;

use tokio::runtime::{self, Runtime};

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
