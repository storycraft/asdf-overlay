use bincode::{Decode, Encode};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::message::{Request, Response};

pub mod client;
pub mod server;

fn create_name(pid: u32) -> String {
    format!("\\\\.\\pipe\\asdf-overlay-{pid}")
}

#[derive(Encode, Decode)]
struct ServerRequest {
    pub id: u32,
    pub req: Request,
}

#[derive(Encode, Decode)]
struct ClientResponse {
    pub id: u32,
    pub body: Response,
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
