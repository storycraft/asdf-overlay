use core::time::Duration;
use std::time::Instant;

use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;

pub(crate) struct ClickState {
    inner: Option<Inner>,
}

impl ClickState {
    pub(crate) const fn new() -> Self {
        Self { inner: None }
    }

    pub(crate) fn get_click_count(&mut self, button: u32, time: Instant) -> u32 {
        let multi_click_time = Duration::from_millis(unsafe { GetDoubleClickTime() } as _);

        match self.inner {
            Some(ref mut inner)
                if inner.last_button == button
                    && time.duration_since(inner.last_click_time) <= multi_click_time =>
            {
                inner.last_click_count += 1;
                inner.last_click_time = time;

                inner.last_click_count
            }

            _ => {
                self.inner = Some(Inner {
                    last_button: button,
                    last_click_count: 1,
                    last_click_time: time,
                });

                1
            }
        }
    }
}

struct Inner {
    last_button: u32,
    last_click_count: u32,
    last_click_time: Instant,
}
