//! Debugging utilities for overlay DLL.
//!
//! Since most GUI applications do not have stdout/stderr consoles, this module provides
//! a `tracing` writer that outputs to the Windows debugger output (OutputDebugString).
//! You can view these outputs using tools like [DebugView](https://docs.microsoft.com/en-us/sysinternals/downloads/debugview).

use parking_lot::{Mutex, MutexGuard};
use std::io::{self, Write};
use tracing::Subscriber;
use tracing_subscriber::{Layer, fmt::MakeWriter, registry::LookupSpan};
use windows::{Win32::System::Diagnostics::Debug::OutputDebugStringW, core::PCWSTR};

pub fn layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_thread_ids(true)
        .with_writer(WinDbgMakeWriter::new())
}

/// A `tracing` writer that outputs to the Windows debugger output (OutputDebugString).
struct WinDbgMakeWriter {
    buf: Mutex<Vec<u16>>,
}

impl WinDbgMakeWriter {
    /// Create a new [`WinDbgMakeWriter`].
    fn new() -> Self {
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

struct WinDbgWriter<'a> {
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
