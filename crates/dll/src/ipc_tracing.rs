mod ipc;

#[cfg(debug_assertions)]
mod dbg;

use tracing::Subscriber;
use tracing_subscriber::{Registry, layer::SubscriberExt};

#[cfg(debug_assertions)]
pub fn subscriber() -> impl Subscriber {
    Registry::default().with(dbg::layer()).with(ipc::layer())
}

#[cfg(not(debug_assertions))]
pub fn subscriber() -> impl Subscriber {
    Registry::default().with(ipc::layer())
}
