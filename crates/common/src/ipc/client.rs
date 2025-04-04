use std::sync::Arc;

use dashmap::DashMap;
use parity_tokio_ipc::{Connection, Endpoint};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    sync::oneshot,
    task::JoinHandle,
};

use crate::message::{Request, Response};

use super::{Frame, create_name};

pub struct IpcClientConn {
    next_id: u32,
    tx: WriteHalf<Connection>,
    buf: Vec<u8>,
    map: Arc<DashMap<u32, oneshot::Sender<Response>>>,
    read_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcClientConn {
    pub async fn connect(pid: u32) -> anyhow::Result<Self> {
        let name = create_name(pid);

        let (mut rx, tx) = split(Endpoint::connect(name).await?);

        let map = Arc::new(DashMap::<u32, oneshot::Sender<Response>>::new());

        let read_task = tokio::spawn({
            let map = map.clone();

            async move {
                let mut body = Vec::new();
                loop {
                    let frame = Frame::read(&mut rx).await?;
                    body.resize(frame.size as usize, 0_u8);
                    rx.read_exact(&mut body).await?;

                    let res: Response =
                        bincode::decode_from_slice(&body, bincode::config::standard())?.0;

                    if let Some((_, sender)) = map.remove(&frame.id) {
                        _ = sender.send(res);
                    }
                }
            }
        });

        Ok(IpcClientConn {
            next_id: 0,
            tx,
            buf: Vec::new(),
            map,
            read_task,
        })
    }

    pub async fn request(&mut self, req: &Request) -> anyhow::Result<Response> {
        Ok(self.send(req).await?.await?)
    }

    async fn send(&mut self, req: &Request) -> anyhow::Result<oneshot::Receiver<Response>> {
        let id = self.next_id;
        self.next_id += 1;

        bincode::encode_into_std_write(req, &mut self.buf, bincode::config::standard())?;

        Frame {
            id,
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

impl Drop for IpcClientConn {
    fn drop(&mut self) {
        self.read_task.abort();
    }
}
