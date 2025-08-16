use std::sync::Arc;

use arc_swap::ArcSwapOption;
use asdf_overlay_event::ClientEvent;

static CURRENT: ArcSwapOption<OverlayEventSink> = ArcSwapOption::const_empty();

pub struct OverlayEventSink {
    sink: Box<dyn Fn(ClientEvent) + Send + Sync>,
}

impl OverlayEventSink {
    #[inline]
    pub fn connected() -> bool {
        CURRENT.load().is_some()
    }

    #[inline]
    pub(crate) fn emit(event: ClientEvent) {
        if let Some(ref this) = *CURRENT.load() {
            (this.sink)(event);
        }
    }

    pub fn set(sink: impl Fn(ClientEvent) + Send + Sync + 'static) {
        CURRENT.store(Some(Arc::new(Self {
            sink: Box::new(sink),
        })));
    }

    pub fn clear() {
        CURRENT.store(None);
    }
}
