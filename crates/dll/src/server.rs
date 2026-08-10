//! Server-side IPC implementation.
//! * Using [`IpcServerConn`] one can read requests from the client and reply to them.
//! * Using [`IpcClientEventEmitter`] one can emit events to the client.

use asdf_overlay_common::{
    event::OverlayEvent,
    ipc::{ClientRequest, Frame, ServerToClientPacket},
    request::Request,
};
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, split},
    net::windows::named_pipe::NamedPipeServer,
    sync::mpsc::{UnboundedSender, unbounded_channel},
};

/// IPC server implementatation.
pub struct IpcServerConn {
    rx: ReadHalf<NamedPipeServer>,
    buf: Vec<u8>,
    chan: UnboundedSender<ServerToClientPacket>,
}

impl IpcServerConn {
    /// Initiate a new [`IpcServerConn`] instance with the given named pipe server.
    pub async fn new(server: NamedPipeServer) -> anyhow::Result<Self> {
        let (rx, mut tx) = split(server);
        let (chan_tx, mut chan_rx) = unbounded_channel();

        tokio::spawn({
            async move {
                let mut buf = vec![];
                while let Some(packet) = chan_rx.recv().await {
                    buf.clear();
                    rmp_serde::encode::write(&mut buf, &packet)?;

                    Frame {
                        size: buf.len() as u32,
                    }
                    .write(&mut tx)
                    .await?;
                    tx.write_all(&buf).await?;

                    tx.flush().await?;
                }

                Ok::<_, anyhow::Error>(())
            }
        });

        Ok(Self {
            rx,
            buf: vec![],
            chan: chan_tx,
        })
    }

    /// Create new [`IpcClientEventEmitter`] instance for emitting events to the client.
    pub fn create_emitter(&self) -> IpcClientEventEmitter {
        IpcClientEventEmitter {
            inner: self.chan.clone(),
        }
    }

    /// Read one request from the client.
    pub async fn recv(&mut self) -> anyhow::Result<(u32, Request)> {
        let frame = Frame::read(&mut self.rx).await?;
        self.buf.resize(frame.size as usize, 0_u8);
        self.rx.read_exact(&mut self.buf).await?;

        let packet: ClientRequest = rmp_serde::from_slice(&self.buf)?;
        Ok((packet.id, packet.req))
    }

    /// Reply to the client with the given request ID and data.
    pub fn reply<T: Serialize>(&mut self, id: u32, response: T) -> anyhow::Result<()> {
        _ = self.chan.send(ServerToClientPacket::Response {
            id,
            payload: rmp_serde::to_vec(&response)?,
        });

        Ok(())
    }
}

/// Event emitter for IPC server.
#[derive(Clone)]
pub struct IpcClientEventEmitter {
    inner: UnboundedSender<ServerToClientPacket>,
}

impl IpcClientEventEmitter {
    /// Emit an event to the client.
    pub fn emit(&self, event: OverlayEvent) -> anyhow::Result<()> {
        self.inner.send(ServerToClientPacket::Event(event))?;

        Ok(())
    }
}
