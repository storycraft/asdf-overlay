use std::{sync::Arc, thread};

use asdf_overlay_window::Backends;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let el = EventLoop::new()?;

    let backends = Arc::new(Backends::new(unsafe { GetModuleHandleW(None) }?.0 as _)?);
    thread::spawn({
        let backends = backends.clone();
        move || {
            while let Some(event) = backends.recv() {
                eprintln!("Backend event: {event:?}");
            }
        }
    });

    el.run_app(&mut App {
        win: None,
        backends,
    })?;
    Ok(())
}

struct App {
    win: Option<Window>,
    backends: Arc<Backends>,
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
        match event {
            WindowEvent::CloseRequested => {
                el.exit();
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Enter),
                        ..
                    },
                ..
            } => {
                eprintln!("Blocking input.");
                self.backends.block_input();
            }

            _ => {}
        }
    }
}
