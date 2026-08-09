use std::thread;

use asdf_overlay_window::Backends;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let el = EventLoop::new()?;

    let backends = Backends::new(unsafe { GetModuleHandleW(None) }?.0 as _)?;
    thread::spawn(move || {
        backends.block_input();

        while let Some(event) = backends.recv() {
            eprintln!("Backend event: {event:?}");
        }
    });

    el.run_app(&mut App { win: None })?;
    Ok(())
}

struct App {
    win: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        self.win = Some(
            el.create_window(WindowAttributes::default())
                .expect("failed to create example window"),
        );
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.win.take();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            el.exit();
        }
    }
}
