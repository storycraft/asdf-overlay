use std::sync::Arc;

use asdf_overlay_window::Backends;
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, DeviceEvents, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let el = EventLoop::new()?;
    el.listen_device_events(DeviceEvents::Always);

    let backends = Arc::new(Backends::new(|event| {
        eprintln!("Backend event: {event:?}");
    })?);

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

    fn device_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        eprintln!("Device event: {event:?}");
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
