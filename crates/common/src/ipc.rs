//! Common types and utilities for IPC communication between the overlay client and server.

use core::error::Error;

use serde::{Deserialize, Serialize};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{event::OverlayEvent, request::Request};

/// Return the named-pipe path for a process and the low 32 bits of its module handle.
pub fn create_ipc_addr(pid: u32, module_handle: u32) -> String {
    format!("\\\\.\\pipe\\asdf-overlay-{pid}-{module_handle}")
}

/// Describes a request sent from the client to the server.
#[derive(Serialize, Deserialize)]
pub struct ClientRequest {
    /// Unique identifier for matching responses.
    pub id: u32,

    /// The actual request data.
    pub req: Request,
}

/// Common ipc result type.
#[derive(Serialize, Deserialize)]
pub enum ResponseResult<T> {
    Ok(T),
    Err(String),
}

impl<E: Error, T> From<E> for ResponseResult<T> {
    fn from(err: E) -> Self {
        ResponseResult::Err(err.to_string())
    }
}

/// Describes a packet sent from server to client.
#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClientPacket {
    /// The packet is a response to a specific request.
    Response { id: u32, payload: Vec<u8> },

    /// The packet is an event notification.
    Event(OverlayEvent),
}

/// An IPC frame header containing a four-byte, big-endian body length.
///
/// The body follows the header and contains exactly `size` bytes.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Size of the frame body in bytes.
    pub size: u32,
}

impl Frame {
    /// Read the header, leaving the body unread.
    ///
    /// No size limit is enforced; validate the length before allocating.
    ///
    /// # Cancellation
    /// Cancelling may consume a partial header.
    pub async fn read(mut r: impl AsyncRead + Unpin) -> io::Result<Self> {
        Ok(Self {
            size: r.read_u32().await?,
        })
    }

    /// Write the header without writing or flushing the body.
    ///
    /// # Cancellation
    /// Cancelling may leave a partial header written.
    pub async fn write(self, mut w: impl AsyncWrite + Unpin) -> io::Result<()> {
        w.write_u32(self.size).await?;
        Ok(())
    }
}
