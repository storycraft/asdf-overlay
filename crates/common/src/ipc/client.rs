use std::process;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, split},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    sync::mpsc::{Sender, channel},
    task::JoinHandle,
};

use crate::message::{Request, Response};

use super::{Frame, create_name};

pub struct IpcClientConn {
    rx: ReadHalf<NamedPipeClient>,
    buf: Vec<u8>,
    chan: Sender<(u32, Response)>,
    write_task: JoinHandle<anyhow::Result<()>>,
}

impl IpcClientConn {
    pub async fn connect() -> anyhow::Result<Self> {
        let name = create_name(process::id());

        let (rx, mut tx) = split(ClientOptions::new().open(name)?);
        let (chan_tx, mut chan_rx) = channel(4);

        let write_task = tokio::spawn({
            async move {
                let mut buf = Vec::new();
                while let Some((id, res)) = chan_rx.recv().await {
                    bincode::encode_into_std_write(res, &mut buf, bincode::config::standard())?;

                    Frame {
                        id,
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

    pub async fn recv(
        &mut self,
        f: impl AsyncFnOnce(Request) -> anyhow::Result<Response>,
    ) -> anyhow::Result<()> {
        let frame = Frame::read(&mut self.rx).await?;
        self.buf.resize(frame.size as usize, 0_u8);
        self.rx.read_exact(&mut self.buf).await?;

        let req: Request = bincode::decode_from_slice(&self.buf, bincode::config::standard())?.0;

        _ = self.chan.send((frame.id, f(req).await?)).await;

        Ok(())
    }

    pub async fn close(self) -> anyhow::Result<()> {
        drop(self.chan);
        self.write_task.await??;

        Ok(())
    }
}
