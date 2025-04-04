use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub mod client;
pub mod server;

fn create_name(pid: u32) -> String {
    format!("\\\\.\\pipe\\asdf-overlay-{pid}")
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    id: u32,
    size: u32,
}

impl Frame {
    async fn read(mut r: impl AsyncRead + Unpin) -> io::Result<Self> {
        Ok(Self {
            id: r.read_u32().await?,
            size: r.read_u32().await?,
        })
    }

    async fn write(self, mut w: impl AsyncWrite + Unpin) -> io::Result<()> {
        w.write_u32(self.id).await?;
        w.write_u32(self.size).await?;
        Ok(())
    }
}
