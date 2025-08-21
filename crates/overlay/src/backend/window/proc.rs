use super::WindowBackend;
use crate::{
    backend::{
        BACKENDS, Backends,
        window::{CursorState, ImeState, WindowProcData, cursor::load_cursor},
    },
    event_sink::OverlayEventSink,
    util::get_client_size,
};
use asdf_overlay_event::{
    ClientEvent, WindowEvent,
    input::{
        ConversionMode, CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, InputEvent,
        InputPosition, KeyboardInput, ScrollAxis,
    },
};
use core::mem;
use parking_lot::MutexGuard;
use scopeguard::defer;
use tracing::trace;
use utf16string::{LittleEndian, WStr, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Globalization::LCIDToLocaleName,
    System::SystemServices::{LOCALE_NAME_MAX_LENGTH, SORT_DEFAULT},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::{
            Ime::{
                self as ime, HIMC, IME_COMPOSITION_STRING, IME_CONVERSION_MODE,
                ImmGetCompositionStringW, ImmGetContext, ImmGetConversionStatus, ImmReleaseContext,
            },
            KeyboardAndMouse::{
                GetDoubleClickTime, GetKeyboardLayout, ReleaseCapture, SetCapture, TME_LEAVE,
                TRACKMOUSEEVENT, TrackMouseEvent,
            },
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, GetMessageTime, SetCursor, WM_NCDESTROY,
            XBUTTON1,
        },
    },
};

#[inline]
fn process_wnd_proc(
    backend: &WindowBackend,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match msg {
        msg::WM_WINDOWPOSCHANGED => {
            let new_size = get_client_size(HWND(backend.hwnd as _)).unwrap();
            let mut render = backend.render.lock();
            if render.window_size != new_size {
                render.window_size = new_size;

                OverlayEventSink::emit(ClientEvent::Window {
                    id: backend.hwnd,
                    event: WindowEvent::Resized {
                        width: new_size.0,
                        height: new_size.1,
                    },
                });
            }
            drop(render);
            backend.recalc_position();
        }

        // set cursor in client area
        msg::WM_SETCURSOR
            if {
                let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
                // check if cursor is on client
                area == 1
            } =>
        {
            let proc = backend.proc.lock();
            if proc.input_blocking() {
                unsafe { SetCursor(proc.blocking_cursor.and_then(load_cursor)) };
                return Some(LRESULT(1));
            }
        }

        // stop input capture when user request to
        msg::WM_CLOSE => {
            let input_blocking = backend.proc.lock().input_blocking();
            if input_blocking {
                backend.block_input(false);
                return Some(LRESULT(0));
            }
        }

        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Left, state, lparam);
        }

        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Middle, state, lparam);
        }

        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Right, state, lparam);
        }

        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONDBLCLK => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<1>(
                backend.hwnd,
                proc,
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                state,
                lparam,
            );
        }

        msg::WM_LBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Left,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_MBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Middle,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_RBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Right,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_XBUTTONUP => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            return cursor_event::<1>(
                backend.hwnd,
                backend.proc.lock(),
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                CursorInputState::Released,
                lparam,
            );
        }

        Controls::WM_MOUSELEAVE => {
            let mut proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            proc.cursor_state = CursorState::Outside;
            OverlayEventSink::emit(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Leave,
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_MOUSEMOVE => {
            let mut proc = backend.proc.lock();
            if proc.listening_cursor() {
                let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

                match proc.cursor_state {
                    CursorState::Inside(ref mut old_x, ref mut old_y) => {
                        *old_x = x;
                        *old_y = y;
                    }
                    CursorState::Outside => {
                        proc.cursor_state = CursorState::Inside(x, y);
                        OverlayEventSink::emit(cursor_input(
                            backend.hwnd,
                            proc.position,
                            lparam,
                            CursorEvent::Enter,
                        ));

                        // track for leave event
                        _ = unsafe {
                            TrackMouseEvent(&mut TRACKMOUSEEVENT {
                                cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                                dwFlags: TME_LEAVE,
                                hwndTrack: HWND(backend.hwnd as _),
                                dwHoverTime: HOVER_DEFAULT,
                            })
                        };
                    }
                }

                OverlayEventSink::emit(cursor_input(
                    backend.hwnd,
                    proc.position,
                    lparam,
                    CursorEvent::Move,
                ));
            }
        }

        msg::WM_MOUSEWHEEL => {
            let proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayEventSink::emit(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_MOUSEHWHEEL => {
            let proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayEventSink::emit(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                },
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_APPCOMMAND => {
            let input_blocking = backend.proc.lock().input_blocking();
            if input_blocking {
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        // block other keyboard, mouse event
        msg::WM_CAPTURECHANGED
        | msg::WM_ACTIVATE
        | msg::WM_ACTIVATEAPP
        | msg::WM_SETFOCUS
        | msg::WM_KILLFOCUS
        | msg::WM_POINTERUPDATE
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR
        | msg::WM_IME_REQUEST => {
            let proc = backend.proc.lock();
            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_INPUTLANGCHANGEREQUEST => {
            let input_blocking = backend.proc.lock().input_blocking();
            if input_blocking {
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        msg::WM_IME_NOTIFY => {
            let proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            if wparam.0 as u32 == ime::IMN_SETCONVERSIONMODE {
                OverlayEventSink::emit(keyboard_input(
                    backend.hwnd,
                    KeyboardInput::Ime(Ime::ConversionChanged(with_himc(
                        backend.hwnd,
                        ime_conversion_mode,
                    ))),
                ))
            }

            if proc.input_blocking() {
                drop(proc);
                return Some(LRESULT(0));
            }
        }

        msg::WM_INPUTLANGCHANGE => {
            let proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            if let Some(lang) = get_lang_id_locale(lparam.0 as u16) {
                OverlayEventSink::emit(keyboard_input(
                    backend.hwnd,
                    KeyboardInput::Ime(Ime::Changed(lang)),
                ));
            }

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_SETCONTEXT => {
            let proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            let lang_id = unsafe { GetKeyboardLayout(0) }.0 as u16;
            OverlayEventSink::emit(keyboard_input(
                backend.hwnd,
                KeyboardInput::Ime(if wparam.0 != 0 {
                    Ime::Enabled {
                        lang: get_lang_id_locale(lang_id).unwrap_or_else(|| "en".to_string()),
                        conversion: with_himc(backend.hwnd, ime_conversion_mode),
                    }
                } else {
                    Ime::Disabled
                }),
            ));

            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(
                        HWND(backend.hwnd as _),
                        msg,
                        wparam,
                        // Disable composition, candinate window
                        LPARAM(0),
                    )
                });
            }
        }

        msg::WM_IME_STARTCOMPOSITION => {
            let mut proc = backend.proc.lock();
            proc.ime = ImeState::Enabled;
            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        msg::WM_IME_COMPOSITION => {
            let mut proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            if proc.ime != ImeState::Disabled {
                with_himc(backend.hwnd, |himc| {
                    let comp = IME_COMPOSITION_STRING(lparam.0 as _);

                    // cancelled
                    if comp == IME_COMPOSITION_STRING(0) {
                        OverlayEventSink::emit(keyboard_input(
                            backend.hwnd,
                            KeyboardInput::Ime(Ime::Commit(String::new())),
                        ));
                    }

                    if comp.contains(ime::GCS_RESULTSTR) {
                        if let Some(text) = get_ime_string(himc, ime::GCS_RESULTSTR) {
                            proc.ime = ImeState::Enabled;
                            OverlayEventSink::emit(keyboard_input(
                                backend.hwnd,
                                KeyboardInput::Ime(Ime::Commit(text.to_utf8())),
                            ));
                        }
                    }

                    if comp.contains(ime::GCS_COMPSTR | ime::GCS_COMPATTR | ime::GCS_CURSORPOS) {
                        let caret = if !comp.contains(IME_COMPOSITION_STRING(ime::CS_NOMOVECARET))
                            && comp.contains(ime::GCS_CURSORPOS)
                        {
                            unsafe {
                                ImmGetCompositionStringW(himc, ime::GCS_CURSORPOS, None, 0) as usize
                            }
                        } else {
                            0
                        };

                        if let Some(text) = get_ime_string(himc, ime::GCS_COMPSTR) {
                            proc.ime = ImeState::Compose;

                            OverlayEventSink::emit(keyboard_input(
                                backend.hwnd,
                                KeyboardInput::Ime(Ime::Compose {
                                    text: text.to_utf8(),
                                    caret,
                                }),
                            ));
                        }
                    }
                });
            }

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_ENDCOMPOSITION => {
            let mut proc = backend.proc.lock();
            let ime = proc.ime;
            proc.ime = ImeState::Disabled;

            if ime == ImeState::Compose {
                let hwnd = HWND(backend.hwnd as _);
                let himc = unsafe { ImmGetContext(hwnd) };
                defer!(unsafe {
                    _ = ImmReleaseContext(hwnd, himc);
                });
                if let Some(text) = get_ime_string(himc, ime::GCS_RESULTSTR) {
                    OverlayEventSink::emit(keyboard_input(
                        backend.hwnd,
                        KeyboardInput::Ime(Ime::Commit(text.to_utf8())),
                    ));
                }
            }

            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        _ => {}
    }
    None
}

#[tracing::instrument]
pub(crate) unsafe extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc called");

    defer!({
        // cleanup backend
        if msg == WM_NCDESTROY {
            trace!("cleanup hwnd: {:?}", hwnd);
            Backends::remove_backend(hwnd);
        }
    });

    let backend = BACKENDS.map.get(&(hwnd.0 as u32)).unwrap();
    if let Some(ret) = process_wnd_proc(&backend, msg, wparam, lparam) {
        return ret;
    }
    let original_proc = backend.original_proc;
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn cursor_event<const BLOCK_RESULT: isize>(
    hwnd: u32,
    proc: MutexGuard<WindowProcData>,
    action: CursorAction,
    state: CursorInputState,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if !proc.listening_cursor() {
        return None;
    }

    OverlayEventSink::emit(cursor_input(
        hwnd,
        proc.position,
        lparam,
        CursorEvent::Action { action, state },
    ));

    if proc.input_blocking() {
        // prevent deadlock
        drop(proc);
        match state {
            CursorInputState::Pressed { .. } => unsafe {
                SetCapture(HWND(hwnd as _));
            },
            CursorInputState::Released => unsafe {
                _ = ReleaseCapture();
            },
        }

        Some(LRESULT(BLOCK_RESULT))
    } else {
        None
    }
}

#[inline]
fn check_double_click(proc: &mut WindowProcData) -> bool {
    proc.update_click_time(unsafe { GetMessageTime() }) <= unsafe { GetDoubleClickTime() }
}

#[inline]
fn cursor_input(id: u32, position: (i32, i32), lparam: LPARAM, event: CursorEvent) -> ClientEvent {
    let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

    let window = InputPosition {
        x: x as _,
        y: y as _,
    };
    let surface = InputPosition {
        x: window.x - position.0,
        y: window.y - position.1,
    };
    ClientEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event,
            client: surface,
            window,
        })),
    }
}

#[inline(always)]
fn keyboard_input(id: u32, input: KeyboardInput) -> ClientEvent {
    ClientEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Keyboard(input)),
    }
}

#[inline]
fn with_himc<R>(hwnd: u32, f: impl FnOnce(HIMC) -> R) -> R {
    let hwnd = HWND(hwnd as _);
    let himc = unsafe { ImmGetContext(hwnd) };
    defer!(unsafe {
        _ = ImmReleaseContext(hwnd, himc);
    });

    f(himc)
}

fn get_ime_string(himc: HIMC, comp: IME_COMPOSITION_STRING) -> Option<WString<LittleEndian>> {
    let byte_size = unsafe { ImmGetCompositionStringW(himc, comp, None, 0) };
    if byte_size >= 0 {
        let mut buf = vec![0_u8; byte_size as usize];

        unsafe {
            ImmGetCompositionStringW(himc, comp, Some(buf.as_mut_ptr().cast()), buf.len() as _)
        };

        WString::from_utf16le(buf).ok()
    } else {
        None
    }
}

fn get_lang_id_locale(lang_id: u16) -> Option<String> {
    let lcid = const { SORT_DEFAULT << 16 } | lang_id as u32;

    let mut buf = [0_u16; LOCALE_NAME_MAX_LENGTH as usize];
    let size = unsafe { LCIDToLocaleName(lcid, Some(&mut buf), 0) };
    if size > 0 {
        Some(
            WStr::from_utf16le(bytemuck::cast_slice::<_, u8>(&buf[..(size - 1) as usize]))
                .ok()?
                .to_utf8(),
        )
    } else {
        None
    }
}

fn ime_conversion_mode(himc: HIMC) -> ConversionMode {
    let mut raw_mode = IME_CONVERSION_MODE(0);
    _ = unsafe { ImmGetConversionStatus(himc, Some(&mut raw_mode), None) };

    let mut mode = ConversionMode::empty();
    if raw_mode.contains(ime::IME_CMODE_NATIVE) {
        mode |= ConversionMode::NATIVE;
    }
    if raw_mode.contains(ime::IME_CMODE_FULLSHAPE) {
        mode |= ConversionMode::FULLSHAPE;
    }
    if raw_mode.contains(ime::IME_CMODE_NOCONVERSION) {
        mode |= ConversionMode::NO_CONVERSION;
    }
    if raw_mode.contains(ime::IME_CMODE_HANJACONVERT) {
        mode |= ConversionMode::HANJA_CONVERT;
    }
    if raw_mode.contains(ime::IME_CMODE_KATAKANA) {
        mode |= ConversionMode::KATAKANA;
    }
    mode
}
