use bincode::Encode;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, split},
    net::windows::named_pipe::NamedPipeServer,
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::JoinHandle,
};

use super::{ClientResponse, ClientToServerPacket, Frame, ServerRequest};
use crate::{event::ClientEvent, request::Request};

pub struct IpcServerConn {
    rx: ReadHalf<NamedPipeServer>,
    buf: Vec<u8>,
    chan: UnboundedSender<ClientToServerPacket>,
    write_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcServerConn {
    pub async fn new(server: NamedPipeServer) -> anyhow::Result<Self> {
        let (rx, mut tx) = split(server);
        let (chan_tx, mut chan_rx) = unbounded_channel();

        let write_task = tokio::spawn({
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

                Ok(())
            }
        });

        Ok(Self {
            rx,
            buf: Vec::new(),
            chan: chan_tx,
            write_task,
        })
    }

    pub fn create_emitter(&self) -> IpcClientEventEmitter {
        IpcClientEventEmitter {
            inner: self.chan.clone(),
        }
    }

    pub async fn recv(&mut self) -> anyhow::Result<(u32, Request)> {
        let frame = Frame::read(&mut self.rx).await?;
        self.buf.resize(frame.size as usize, 0_u8);
        self.rx.read_exact(&mut self.buf).await?;

        let packet: ServerRequest =
            bincode::decode_from_slice(&self.buf, bincode::config::standard())?.0;
        Ok((packet.id, packet.req))
    }

    pub fn reply(&mut self, id: u32, data: impl Encode) -> anyhow::Result<()> {
        _ = self
            .chan
            .send(ClientToServerPacket::Response(ClientResponse {
                id,
                data: bincode::encode_to_vec(data, bincode::config::standard())?,
            }));

        Ok(())
    }

    pub async fn close(self) -> anyhow::Result<()> {
        drop(self.chan);
        self.write_task.await??;

        Ok(())
    }
}

pub struct IpcClientEventEmitter {
    inner: UnboundedSender<ClientToServerPacket>,
}

impl IpcClientEventEmitter {
    pub fn emit(&self, event: ClientEvent) -> anyhow::Result<()> {
        self.inner.send(ClientToServerPacket::Event(event))?;

        Ok(())
    }
}
