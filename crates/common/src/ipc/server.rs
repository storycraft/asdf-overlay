use std::sync::Arc;

use anyhow::Context;
use dashmap::DashMap;
use scopeguard::defer;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::oneshot,
    task::JoinHandle,
};

use crate::{
    ipc::ClientResponse,
    message::{Request, Response},
};

use super::{Frame, ServerRequest, create_name};

pub fn create_ipc_server(pid: u32) -> anyhow::Result<NamedPipeServer> {
    let name = create_name(pid);

    Ok(ServerOptions::new()
        .first_pipe_instance(true)
        .create(name)?)
}

pub struct IpcServerConn {
    next_id: u32,
    tx: WriteHalf<NamedPipeServer>,
    buf: Vec<u8>,
    map: Arc<DashMap<u32, oneshot::Sender<Response>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcServerConn {
    pub async fn connect(server: NamedPipeServer) -> anyhow::Result<Self> {
        server.connect().await?;

        let (mut rx, tx) = split(server);
        let map = Arc::new(DashMap::<u32, oneshot::Sender<Response>>::new());

        let read_task = tokio::spawn({
            let map = map.clone();

            async move {
                let mut body = Vec::new();
                defer!(map.clear());

                loop {
                    let frame = Frame::read(&mut rx).await?;
                    body.resize(frame.size as usize, 0_u8);
                    rx.read_exact(&mut body).await?;

                    let res: ClientResponse =
                        bincode::decode_from_slice(&body, bincode::config::standard())?.0;

                    if let Some((_, sender)) = map.remove(&res.id) {
                        _ = sender.send(res.body);
                    }
                }
            }
        });

        Ok(IpcServerConn {
            next_id: 0,
            tx,
            buf: Vec::new(),
            map,
            read_task,
        })
    }

    pub async fn request(&mut self, req: Request) -> anyhow::Result<Response> {
        self.send(req)
            .await
            .context("failed to send request")?
            .await
            .context("failed to receive response")
    }

    async fn send(&mut self, req: Request) -> anyhow::Result<oneshot::Receiver<Response>> {
        let id = self.next_id;
        self.next_id += 1;

        bincode::encode_into_std_write(
            ServerRequest { id, req: req },
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

impl Drop for IpcServerConn {
    fn drop(&mut self) {
        self.read_task.abort();
    }
}
