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

    pub(crate) fn get_click_count(&mut self, x: i32, y: i32, button: u32, time: Instant) -> u32 {
        fn is_consecutive(inner: &Inner, x: i32, y: i32, button: u32, time: Instant) -> bool {
            const MAX_DISTANCE: u32 = 4;

            if inner.button != button {
                return false;
            }

            if inner.x.abs_diff(x) > MAX_DISTANCE / 2 || inner.y.abs_diff(y) > MAX_DISTANCE / 2 {
                return false;
            }

            let multi_click_time = Duration::from_millis(unsafe { GetDoubleClickTime() } as _);
            time.duration_since(inner.last_click_time) <= multi_click_time
        }

        match self.inner {
            Some(ref mut inner) if is_consecutive(inner, x, y, button, time) => {
                inner.click_count += 1;
                inner.last_click_time = time;

                inner.click_count
            }

            _ => {
                self.inner = Some(Inner {
                    x,
                    y,
                    button,
                    click_count: 1,
                    last_click_time: time,
                });

                1
            }
        }
    }
}

struct Inner {
    x: i32,
    y: i32,
    button: u32,
    click_count: u32,
    last_click_time: Instant,
}
