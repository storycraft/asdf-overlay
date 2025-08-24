//! Client side IPC connection and event stream implementation.
//!
//! Provides interfaces for sending requests via ipc and receive events.

use std::sync::{Arc, Weak};

use anyhow::{Context as AnyhowContext, bail};
use asdf_overlay_common::{
    ipc::{ClientRequest, Frame, ServerToClientPacket},
    request::{Request, WindowRequestItem},
};
use asdf_overlay_event::ServerEvent;
use bincode::Decode;
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
    buf: Vec<u8>,
    map: Weak<DashMap<u32, oneshot::Sender<Vec<u8>>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcClientConn {
    /// Create a new [`IpcClientConn`] and [`IpcClientEventStream`] from a connected named pipe client.
    pub async fn new(client: NamedPipeClient) -> anyhow::Result<(Self, IpcClientEventStream)> {
        let (mut rx, tx) = split(client);

        let map = Arc::new(DashMap::<u32, oneshot::Sender<Vec<u8>>>::new());
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let read_task = tokio::spawn({
            let map = map.clone();

            async move {
                let mut body = Vec::new();
                loop {
                    let frame = Frame::read(&mut rx).await?;
                    body.resize(frame.size as usize, 0_u8);
                    rx.read_exact(&mut body).await?;

                    let packet: ServerToClientPacket =
                        bincode::decode_from_slice(&body, bincode::config::standard())?.0;

                    match packet {
                        ServerToClientPacket::Response(res) => {
                            if let Some((_, sender)) = map.remove(&res.id) {
                                _ = sender.send(res.data);
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
            buf: Vec::new(),
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
    async fn request<Response: Decode<()>>(&mut self, req: Request) -> anyhow::Result<Response> {
        let data = self
            .send(req)
            .await
            .context("failed to send request")?
            .await
            .context("failed to receive response")?;

        let (response, read) =
            bincode::decode_from_slice::<Response, _>(&data, bincode::config::standard())?;
        let remaining = data.len() - read;
        if remaining != 0 {
            bail!(
                "Response is {} bytes but only {read} bytes read",
                data.len()
            );
        }

        Ok(response)
    }

    /// Send a request without waiting for the response.
    /// Returns a oneshot receiver that can be used to receive the response data.
    async fn send(&mut self, req: Request) -> anyhow::Result<oneshot::Receiver<Vec<u8>>> {
        let Some(map) = self.map.upgrade() else {
            bail!("connection closed");
        };

        let id = self.next_id;
        self.next_id += 1;

        bincode::encode_into_std_write(
            ClientRequest { id, req },
            &mut self.buf,
            bincode::config::standard(),
        )?;

        Frame {
            size: self.buf.len() as _,
        }
        .write(&mut self.tx)
        .await?;

        let (tx, rx) = oneshot::channel();
        map.insert(id, tx);
        self.tx.write_all(&self.buf).await?;

        self.tx.flush().await?;

        self.buf.clear();

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
    /// * Returns `true` if the request is successful.
    /// * Returns an error if the connection is closed or the request fails.
    pub async fn request(&mut self, req: impl WindowRequestItem) -> anyhow::Result<bool> {
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
    inner: mpsc::UnboundedReceiver<ServerEvent>,
}

impl IpcClientEventStream {
    /// Receive the next event.
    /// Returns `None` if the connection is closed.
    #[inline]
    pub async fn recv(&mut self) -> Option<ServerEvent> {
        self.inner.recv().await
    }
}
