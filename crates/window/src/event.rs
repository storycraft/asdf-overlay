use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_window_event::Event;

static CURRENT: ArcSwapOption<EventSink> = ArcSwapOption::const_empty();

pub struct EventSink {
    sink: Box<dyn Fn(Event) + Send + Sync>,
}

impl EventSink {
    pub fn set(sink: impl Fn(Event) + Send + Sync + 'static) {
        CURRENT.store(Some(Arc::new(Self {
            sink: Box::new(sink),
        })));
    }

    #[inline]
    pub fn emit(event: Event) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

    #[inline]
    pub fn clear() {
        CURRENT.store(None);
    }
}
