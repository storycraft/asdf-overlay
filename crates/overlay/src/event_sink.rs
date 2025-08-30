//! Provides [`OverlayEventSink`] for receiving [`OverlayEvent`] from overlay system.

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_event::OverlayEvent;

/// Global [`OverlayEventSink`] instance.
static CURRENT: ArcSwapOption<OverlayEventSink> = ArcSwapOption::const_empty();

/// Event sink for overlay system.
pub struct OverlayEventSink {
    sink: Box<dyn Fn(OverlayEvent) + Send + Sync>,
}

impl OverlayEventSink {
    #[inline]
    /// Check if there are currently set event sink.
    pub fn connected() -> bool {
        CURRENT.load().is_some()
    }

    #[inline]
    /// Emit [`OverlayEvent`] to event sink. If one exists.
    pub(crate) fn emit(event: OverlayEvent) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

    /// Set event sink function.
    ///
    /// Overlay will not detect windows or render before setting it.
    pub fn set(sink: impl Fn(OverlayEvent) + Send + Sync + 'static) {
        CURRENT.store(Some(Arc::new(Self {
            sink: Box::new(sink),
        })));
    }

    /// Clear event sink function.
    pub fn clear() {
        CURRENT.store(None);
    }
}
