//! Receive overlay [`Event`] values through the global [`OverlayEventSink`].
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_event::Event;

/// Global [`OverlayEventSink`] instance.
static CURRENT: ArcSwapOption<OverlayEventSink> = ArcSwapOption::const_empty();

/// The process-wide overlay event callback.
///
/// Callbacks run synchronously, potentially concurrently, and must not reenter
/// surface registry operations. Queue such work to avoid deadlocks. Panics propagate
/// to the emitting code.
pub struct OverlayEventSink {
    sink: Box<dyn Fn(Event) + Send + Sync>,
}

impl OverlayEventSink {
    #[inline]
    /// Return whether an event callback is installed.
    pub fn connected() -> bool {
        CURRENT.load().is_some()
    }

    #[inline]
    #[doc(hidden)]
    /// Send an event to the callback, if installed.
    pub fn emit(event: Event) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

    /// Replace the callback and enable surface detection and rendering.
    ///
    /// Existing surfaces are not replayed.
    pub fn set(sink: impl Fn(Event) + Send + Sync + 'static) {
        CURRENT.store(Some(Arc::new(Self {
            sink: Box::new(sink),
        })));
    }

    /// Stop future callbacks without resetting surfaces or hooks.
    ///
    /// An in-progress callback may finish after this returns.
    pub fn clear() {
        CURRENT.store(None);
    }
}
