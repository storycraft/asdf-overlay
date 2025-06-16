use std::sync::{Arc, Weak};

use anyhow::{Context as AnyhowContext, bail};
use bincode::Decode;
use dashmap::DashMap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    net::windows::named_pipe::NamedPipeClient,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    event::ClientEvent,
    ipc::ClientToServerPacket,
    request::{
        BlockInput, ListenInput, Request, SetAnchor, SetBlockingCursor, SetMargin, SetPosition,
        UpdateSharedHandle,
    },
};

use super::{Frame, ServerRequest};

pub struct IpcClientConn {
    next_id: u32,
    tx: WriteHalf<NamedPipeClient>,
    buf: Vec<u8>,
    map: Weak<DashMap<u32, oneshot::Sender<Vec<u8>>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcClientConn {
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
        let Some(map) = self.map.upgrade() else {
            bail!("connection closed");
        };

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
        map.insert(id, tx);
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
        impl IpcClientConn {
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

    /// Listen input events
    listen_input(ListenInput) -> bool;

    /// Block input events from reaching window and listen all input events
    block_input(BlockInput) -> bool;

    /// Set cursor of a window being input captured
    set_blocking_cursor(SetBlockingCursor) -> bool;

    /// Update overlay surface
    update_shtex(UpdateSharedHandle) -> ();
}

impl Drop for IpcClientConn {
    fn drop(&mut self) {
        self.read_task.abort();
    }
}

pub struct IpcClientEventStream {
    inner: mpsc::UnboundedReceiver<ClientEvent>,
}

impl IpcClientEventStream {
    #[inline]
    pub async fn recv(&mut self) -> Option<ClientEvent> {
        self.inner.recv().await
    }
}
