//! Receive overlay [`Event`] values through the global [`OverlayEventSink`].
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_event::Event;

/// Global [`OverlayEventSink`] instance.
static CURRENT: ArcSwapOption<OverlayEventSink> = ArcSwapOption::const_empty();

/// Event sink for overlay system.
pub struct OverlayEventSink {
    sink: Box<dyn Fn(Event) + Send + Sync>,
}

impl OverlayEventSink {
    #[inline]
    /// Check if there are currently set event sink.
    pub fn connected() -> bool {
        CURRENT.load().is_some()
    }

    #[inline]
    #[doc(hidden)]
    /// Emit [`Event`] to event sink. If one exists.
    pub fn emit(event: Event) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

    /// Set event sink function.
    ///
    /// Enables surface detection and rendering, replacing any previous sink.
    /// Existing surfaces are not replayed. The callback runs synchronously on
    /// emitting threads, potentially concurrently and while internal locks are
    /// held. Keep it short and queue work that accesses the surface registry to
    /// avoid reentrant locking. A callback panic propagates into the emitting code.
    pub fn set(sink: impl Fn(Event) + Send + Sync + 'static) {
        CURRENT.store(Some(Arc::new(Self {
            sink: Box::new(sink),
        })));
    }

    /// Remove the sink for future emissions without resetting surfaces or hooks.
    ///
    /// A callback already in progress may finish after this returns.
    pub fn clear() {
        CURRENT.store(None);
    }
}
