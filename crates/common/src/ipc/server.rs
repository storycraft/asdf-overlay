use core::{
    pin::Pin,
    task::{Context, Poll},
};
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, bail};
use bincode::Decode;
use dashmap::DashMap;
use futures_core::Stream;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    net::windows::named_pipe::NamedPipeServer,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    event::ClientEvent,
    ipc::ClientToServerPacket,
    request::{GetSize, Request, SetAnchor, SetMargin, SetPosition, UpdateSharedHandle},
};

use super::{Frame, ServerRequest};

pub struct IpcServerConn {
    next_id: u32,
    tx: WriteHalf<NamedPipeServer>,
    buf: Vec<u8>,
    map: Arc<DashMap<u32, oneshot::Sender<Vec<u8>>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcServerConn {
    pub async fn connect(server: NamedPipeServer) -> anyhow::Result<(Self, IpcServerEventStream)> {
        server.connect().await?;

        let (mut rx, tx) = split(server);
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

                    let packet: ClientToServerPacket =
                        bincode::decode_from_slice(&body, bincode::config::standard())?.0;

                    match packet {
                        ClientToServerPacket::Response(res) => {
                            if let Some((_, sender)) = map.remove(&res.id) {
                                _ = sender.send(res.data);
                            }
                        }
                        ClientToServerPacket::Event(event) => {
                            let _ = event_tx.send(event);
                        }
                    }
                }
            }
        });

        let conn = IpcServerConn {
            next_id: 0,
            tx,
            buf: Vec::new(),
            map,
            read_task,
        };

        let stream = IpcServerEventStream { inner: event_rx };

        Ok((conn, stream))
    }

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

    async fn send(&mut self, req: Request) -> anyhow::Result<oneshot::Receiver<Vec<u8>>> {
        let id = self.next_id;
        self.next_id += 1;

        bincode::encode_into_std_write(
            ServerRequest { id, req },
            &mut self.buf,
            bincode::config::standard(),
        )?;

        Frame {
            size: self.buf.len() as _,
        }
        .write(&mut self.tx)
        .await?;

        let (tx, rx) = oneshot::channel();
        self.map.insert(id, tx);
        self.tx.write_all(&self.buf).await?;

        self.tx.flush().await?;

        self.buf.clear();

        Ok(rx)
    }
}

macro_rules! request_method {
    (
        $(#[$meta:meta])*
        $name:ident($req:ident) -> $res:ty
    ) => {
        $(#[$meta])*
        #[inline(always)]
        pub async fn $name(&mut self, req: $req) -> anyhow::Result<$res> {
            self.request(Request::$req(req)).await
        }
    };
}

macro_rules! requests {
    (
        $(
            $(#[$meta:meta])*
            $name:ident($req:ident) -> $res:ty
        );* $(;)?
    ) => {
        impl IpcServerConn {
            $(
                request_method!(
                    $(#[$meta])*
                    $name($req) -> $res
                );
            )*
        }
    };
}

requests! {
    /// Set overlay position
    set_position(SetPosition) -> ();

    /// Set overlay positioning anchor
    set_anchor(SetAnchor) -> ();

    /// Set overlay margin
    set_margin(SetMargin) -> ();

    /// Get overlay size
    get_size(GetSize) -> Option<(u32, u32)>;

    /// Update overlay surface
    update_shtex(UpdateSharedHandle) -> ();
}

impl Drop for IpcServerConn {
    fn drop(&mut self) {
        self.read_task.abort();
    }
}

pub struct IpcServerEventStream {
    inner: mpsc::UnboundedReceiver<ClientEvent>,
}

impl Stream for IpcServerEventStream {
    type Item = ClientEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}
