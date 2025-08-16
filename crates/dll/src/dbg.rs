use parking_lot::{Mutex, MutexGuard};
use std::io::{self, Write};
use tracing_subscriber::fmt::MakeWriter;
use windows::{Win32::System::Diagnostics::Debug::OutputDebugStringW, core::PCWSTR};

pub struct WinDbgMakeWriter {
    buf: Mutex<Vec<u16>>,
}

impl WinDbgMakeWriter {
    pub fn new() -> Self {
        Self {
            buf: Mutex::new(Vec::new()),
        }
    }
}

impl<'a> MakeWriter<'a> for WinDbgMakeWriter {
    type Writer = WinDbgWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        WinDbgWriter {
            buf: self.buf.lock(),
        }
    }
}

pub struct WinDbgWriter<'a> {
    buf: MutexGuard<'a, Vec<u16>>,
}

impl Write for WinDbgWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(msg) = str::from_utf8(buf) {
            self.buf.extend(msg.encode_utf16());
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for WinDbgWriter<'_> {
    fn drop(&mut self) {
        self.buf.push(0);
        unsafe {
            OutputDebugStringW(PCWSTR(self.buf.as_ptr()));
        }
        self.buf.clear();
    }
}
