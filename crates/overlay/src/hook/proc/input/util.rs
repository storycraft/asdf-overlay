use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::ClipCursor};

use super::HOOK;

pub unsafe fn original_clip_cursor(rect: Option<*const RECT>) -> windows::core::Result<()> {
    unsafe {
        match HOOK.get() {
            Some(hook) => hook.clip_cursor.original_fn()(rect.unwrap_or_default()).ok(),
            None => ClipCursor(rect),
        }
    }
}
