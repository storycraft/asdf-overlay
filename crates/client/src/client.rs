//! Client side IPC connection and event stream implementation.
//!
//! Provides interfaces for sending requests via ipc and receive events.

use std::sync::{Arc, Weak};

use anyhow::{Context as AnyhowContext, bail};
use asdf_overlay_common::{
    event::OverlayEvent,
    ipc::{ClientRequest, Frame, ServerResponse, ServerToClientPacket},
    request::{Request, WindowRequestItem},
};
use bitcode::Buffer;
use dashmap::DashMap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    net::windows::named_pipe::NamedPipeClient,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

/// IPC client connection for handling requests and responses.
pub struct IpcClientConn {
    next_id: u32,
    tx: WriteHalf<NamedPipeClient>,
    buf: Buffer,
    map: Weak<DashMap<u32, oneshot::Sender<ServerResponse>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcClientConn {
    /// Create a new [`IpcClientConn`] and [`IpcClientEventStream`] from a connected named pipe client.
    pub async fn new(client: NamedPipeClient) -> anyhow::Result<(Self, IpcClientEventStream)> {
        let (mut rx, tx) = split(client);

        let map = Arc::new(DashMap::<u32, oneshot::Sender<ServerResponse>>::new());
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let read_task = tokio::spawn({
            let map = map.clone();

            async move {
                let mut body = Vec::new();
                loop {
                    let frame = Frame::read(&mut rx).await?;
                    body.resize(frame.size as usize, 0_u8);
                    rx.read_exact(&mut body).await?;

                    let packet: ServerToClientPacket = bitcode::decode(&body)?;
                    match packet {
                        ServerToClientPacket::Response { id, response } => {
                            if let Some((_, sender)) = map.remove(&id) {
                                _ = sender.send(response);
                            }
                        }

                        ServerToClientPacket::Event(event) => {
                            let _ = event_tx.send(event);
                        }
                    }
                }
            }
        });

        let conn = IpcClientConn {
            next_id: 0,
            tx,
            buf: Buffer::new(),
            map: Arc::downgrade(&map),
            read_task,
        };

        let stream = IpcClientEventStream { inner: event_rx };

        Ok((conn, stream))
    }

    /// Get request interface for a specific window id.
    /// The returned interface can be used to send window-specific requests.
    #[inline]
    pub const fn window(&mut self, id: u32) -> IpcClientConnWindow<'_> {
        IpcClientConnWindow { inner: self, id }
    }

    /// Send a request and wait for the response.
    /// Returns an error if the connection is closed or the request fails.
    pub async fn request(&mut self, req: Request) -> anyhow::Result<ServerResponse> {
        let data = self
            .send(req)
            .await
            .context("failed to send request")?
            .await
            .context("failed to receive response")?;

        Ok(data)
    }

    /// Send a request without waiting for the response.
    /// Returns a oneshot receiver that can be used to receive the response data.
    async fn send(&mut self, req: Request) -> anyhow::Result<oneshot::Receiver<ServerResponse>> {
        let Some(map) = self.map.upgrade() else {
            bail!("connection closed");
        };

        let id = self.next_id;
        self.next_id += 1;

        let data = self.buf.encode(&ClientRequest { id, req });
        Frame {
            size: data.len() as _,
        }
        .write(&mut self.tx)
        .await?;

        let (tx, rx) = oneshot::channel();
        map.insert(id, tx);
        self.tx.write_all(data).await?;

        self.tx.flush().await?;
        Ok(rx)
    }
}

impl Drop for IpcClientConn {
    fn drop(&mut self) {
        self.read_task.abort();
    }
}

/// Request interface for a specific window id.
pub struct IpcClientConnWindow<'a> {
    inner: &'a mut IpcClientConn,
    id: u32,
}

impl IpcClientConnWindow<'_> {
    /// Request any [`WindowRequestItem`].
    pub async fn request(&mut self, req: impl WindowRequestItem) -> anyhow::Result<ServerResponse> {
        self.inner
            .request(Request::Window {
                id: self.id,
                request: req.into(),
            })
            .await
    }
}

/// Event stream for receiving server events.
pub struct IpcClientEventStream {
    inner: mpsc::UnboundedReceiver<OverlayEvent>,
}

impl IpcClientEventStream {
    /// Receive the next event.
    /// Returns `None` if the connection is closed.
    #[inline]
    pub async fn recv(&mut self) -> Option<OverlayEvent> {
        self.inner.recv().await
    }
}
