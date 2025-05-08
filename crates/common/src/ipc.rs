use bincode::{Decode, Encode};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{event::ClientEvent, request::Request};

pub mod client;
pub mod server;

/// Create Windows pipe path using unique name and pid
pub fn create_ipc_path(name: &str, pid: u32) -> String {
    format!("\\\\.\\pipe\\{name}-{pid}")
}

#[derive(Encode, Decode)]
struct ServerRequest {
    pub id: u32,
    pub req: Request,
}

#[derive(Encode, Decode)]
struct ClientResponse {
    pub id: u32,
    pub data: Vec<u8>,
}

#[derive(Encode, Decode)]
enum ClientToServerPacket {
    Response(ClientResponse),
    Event(ClientEvent),
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    size: u32,
}

impl Frame {
    async fn read(mut r: impl AsyncRead + Unpin) -> io::Result<Self> {
        Ok(Self {
            size: r.read_u32().await?,
        })
    }

    async fn write(self, mut w: impl AsyncWrite + Unpin) -> io::Result<()> {
        w.write_u32(self.size).await?;
        Ok(())
    }
}
