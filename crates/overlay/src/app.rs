use anyhow::{Context, bail};
use asdf_overlay_common::{
    ipc::client::IpcClientConn,
    message::{Anchor, Margin, Position, Request, Response},
};
use parking_lot::RwLock;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Console::{AllocConsole, FreeConsole},
            LibraryLoader::GetModuleHandleA,
        },
        UI::WindowsAndMessaging::{
            CS_OWNDC, CreateWindowExA, DefWindowProcW, DestroyWindow, RegisterClassA,
            WINDOW_EX_STYLE, WNDCLASSA, WS_POPUP,
        },
    },
    core::s,
};

use crate::hook::{dx9, dxgi, opengl};

pub struct Overlay {
    position: Position,
    anchor: Anchor,
    margin: Margin,
}

impl Overlay {
    pub fn calc_overlay_position(&self, size: (f32, f32), screen: (u32, u32)) -> (f32, f32) {
        let screen = (screen.0 as f32, screen.1 as f32);

        let margin_left = self.margin.left.resolve(screen.0);
        let margin_top = self.margin.top.resolve(screen.1);

        let outer_width = margin_left + size.0 + self.margin.right.resolve(screen.0);
        let outer_height = margin_top + size.1 + self.margin.bottom.resolve(screen.1);

        let x =
            self.position.x.resolve(screen.0) - self.anchor.x.resolve(outer_width) + margin_left;
        let y =
            self.position.y.resolve(screen.1) - self.anchor.y.resolve(outer_height) + margin_top;

        (x, y)
    }

    pub fn with<R>(f: impl FnOnce(&Self) -> R) -> R {
        f(CURRENT.read().as_ref().expect("Overlay is not initialized"))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        f(&mut *CURRENT
            .write()
            .as_mut()
            .expect("Overlay is not initialized"))
    }
}

static CURRENT: RwLock<Option<Overlay>> = RwLock::new(None);

async fn run_client(mut client: IpcClientConn) -> anyhow::Result<()> {
    loop {
        client
            .recv(async |message| {
                match message {
                    Request::UpdatePosition(position) => {
                        Overlay::with_mut(|overlay| overlay.position = position);
                    }

                    Request::UpdateAnchor(anchor) => {
                        Overlay::with_mut(|overlay| overlay.anchor = anchor);
                    }

                    Request::UpdateMargin(margin) => {
                        Overlay::with_mut(|overlay| overlay.margin = margin);
                    }

                    Request::UpdateBitmap(update_bitmap) => {
                        if let Some(ref mut renderer) = *opengl::RENDERER.lock() {
                            renderer.update_texture(update_bitmap.width, update_bitmap.data);
                        } else if let Some(ref mut renderer) = *dxgi::RENDERER.dx11.lock() {
                            renderer.update_texture(update_bitmap.width, update_bitmap.data);
                        }
                    }

                    Request::Direct(_) => {}
                }

                Ok(Response::Success)
            })
            .await?;
    }
}

pub async fn main() -> anyhow::Result<()> {
    *CURRENT.write() = Some(Overlay {
        position: Position::default(),
        anchor: Anchor::default(),
        margin: Margin::default(),
    });

    let client = IpcClientConn::connect().await?;

    {
        let dummy_hwnd = create_dummy_hwnd().context("cannot create dummy window")?;
        defer!(unsafe {
            _ = DestroyWindow(dummy_hwnd);
        });

        _ = opengl::hook();
        defer!(opengl::cleanup_hook().expect("opengl hook cleanup failed"));

        _ = dxgi::hook(dummy_hwnd);
        defer!(dxgi::cleanup_hook().expect("dxgi hook cleanup failed"));

        _ = dx9::hook(dummy_hwnd);
        defer!(dx9::cleanup_hook().expect("dx9 hook cleanup failed"));
    }

    _ = run_client(client).await;

    Ok(())
}

fn create_dummy_hwnd() -> anyhow::Result<HWND> {
    extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let class_name = s!("asdf-overlay dummy window class");
    let name = s!("asdf-overlay dummy window");

    unsafe {
        let h_instance = GetModuleHandleA(None)?.into();

        RegisterClassA(&WNDCLASSA {
            style: CS_OWNDC,
            hInstance: h_instance,
            lpszClassName: class_name,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        });

        Ok(CreateWindowExA(
            WINDOW_EX_STYLE(0),
            class_name,
            name,
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(h_instance),
            None,
        )?)
    }
}
