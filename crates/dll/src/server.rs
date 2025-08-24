//! Server-side IPC implementation.
//! * Using [`IpcServerConn`] one can read requests from the client and reply to them.
//! * Using [`IpcClientEventEmitter`] one can emit events to the client.

use asdf_overlay_common::{
    ipc::{ClientRequest, Frame, ServerResponse, ServerToClientPacket},
    request::Request,
};
use asdf_overlay_event::ServerEvent;
use bincode::Encode;
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
                let mut buf = Vec::new();
                while let Some(packet) = chan_rx.recv().await {
                    bincode::encode_into_std_write(packet, &mut buf, bincode::config::standard())?;

                    Frame {
                        size: buf.len() as u32,
                    }
                    .write(&mut tx)
                    .await?;
                    tx.write_all(&buf).await?;

                    tx.flush().await?;

                    buf.clear();
                }

                Ok::<_, anyhow::Error>(())
            }
        });

        Ok(Self {
            rx,
            buf: Vec::new(),
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

        let packet: ClientRequest =
            bincode::decode_from_slice(&self.buf, bincode::config::standard())?.0;
        Ok((packet.id, packet.req))
    }

    /// Reply to the client with the given request ID and data.
    pub fn reply(&mut self, id: u32, data: impl Encode) -> anyhow::Result<()> {
        _ = self
            .chan
            .send(ServerToClientPacket::Response(ServerResponse {
                id,
                data: bincode::encode_to_vec(data, bincode::config::standard())?,
            }));

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
    pub fn emit(&self, event: ServerEvent) -> anyhow::Result<()> {
        self.inner.send(ServerToClientPacket::Event(event))?;

        Ok(())
    }
}
