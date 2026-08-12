use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_common::event::OverlayEvent;

static CURRENT: ArcSwapOption<EventSink> = ArcSwapOption::const_empty();

pub struct EventSink {
    sink: Box<dyn Fn(OverlayEvent) + Send + Sync>,
}

impl EventSink {
    #[inline]
    /// Emit [`Event`] to event sink. If one exists.
    pub(crate) fn emit(event: OverlayEvent) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

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
