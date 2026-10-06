use core::mem;

use asdf_overlay_event::{Event, SurfaceEvent};
use once_cell::sync::Lazy;
use scopeguard::defer;
use tracing::{Level, trace};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        self as msg, CallWindowProcA, CallWindowProcW, GWLP_WNDPROC, IsWindowUnicode,
        SetWindowLongPtrA, SetWindowLongPtrW, WNDPROC,
    },
};

use crate::{
    event_sink::OverlayEventSink, hook::opengl::util::get_client_size, surface::Surfaces,
    types::IntDashMap,
};

// HWND -> last WNDPROC
static MAP: Lazy<IntDashMap<u32, WNDPROC>> = Lazy::new(IntDashMap::default);

pub fn install(hwnd: HWND) {
    let key = hwnd.0 as u32;
    if MAP.contains_key(&key) {
        return;
    }

    MAP.entry(key).or_insert_with(|| unsafe {
        let ogl_proc = if IsWindowUnicode(hwnd).as_bool() {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ogl_wnd_proc::<true> as *const () as _)
        } else {
            SetWindowLongPtrA(hwnd, GWLP_WNDPROC, ogl_wnd_proc::<false> as *const () as _)
        } as isize;

        mem::transmute::<isize, WNDPROC>(ogl_proc)
    });
}

#[inline(always)]
fn proc(hwnd: u32, msg: u32, lparam: LPARAM) {
    let msg::WM_WINDOWPOSCHANGED = msg else {
        return;
    };

    let winpos = unsafe { &*(lparam.0 as *const msg::WINDOWPOS) };
    if winpos.flags.0 & msg::SWP_NOSIZE.0 != 0 {
        return;
    }

    let (width, height) = get_client_size(HWND(hwnd as _)).unwrap_or_default();
    for data in super::MAP.iter() {
        if data.hwnd != hwnd {
            continue;
        }

        let key = *data.key() as _;
        Surfaces::state(key, |state| {
            state.resize(width, height);
            OverlayEventSink::emit(Event::Surface {
                id: key,
                event: SurfaceEvent::Resized { width, height },
            });
        });
    }
}

#[tracing::instrument(level = Level::TRACE)]
unsafe extern "system" fn ogl_wnd_proc<const UNICODE: bool>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc opengl hook called");

    let key = hwnd.0 as u32;
    defer!({
        // cleanup map
        if msg == msg::WM_NCDESTROY {
            trace!("cleanup ogl proc hook: {:?}", hwnd);
            MAP.remove(&key);
        }
    });
    let last_wnd_proc = *MAP.get(&key).unwrap();

    proc(key, msg, lparam);
    unsafe {
        if UNICODE {
            CallWindowProcW(last_wnd_proc, hwnd, msg, wparam, lparam)
        } else {
            CallWindowProcA(last_wnd_proc, hwnd, msg, wparam, lparam)
        }
    }
}
