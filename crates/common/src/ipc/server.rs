use core::pin::pin;
use std::process;

use futures::{Stream, StreamExt};
use parity_tokio_ipc::{Endpoint, SecurityAttributes};
use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, split},
    sync::mpsc::{Sender, channel},
    task::JoinHandle,
};

use crate::message::{Request, Response};

use super::{Frame, create_name};

pub fn listen()
-> anyhow::Result<impl Stream<Item = io::Result<IpcServerConn<impl AsyncRead + Unpin>>>> {
    let name = create_name(process::id());

    let mut endpoint = Endpoint::new(name);
    endpoint.set_security_attributes(SecurityAttributes::allow_everyone_create().unwrap());

    Ok(async_stream::try_stream! {
        let incoming = endpoint.incoming()?;
        let mut incoming = pin!(incoming);

        while let Some(incoming) = incoming.next().await.transpose()? {
            yield create_conn(incoming);
        }
    })
}

fn create_conn<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    stream: S,
) -> IpcServerConn<ReadHalf<S>> {
    let (rx, mut tx) = split(stream);
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

    IpcServerConn {
        rx,
        buf: Vec::new(),
        chan: chan_tx,
        write_task,
    }
}

pub struct IpcServerConn<R> {
    rx: R,
    buf: Vec<u8>,
    chan: Sender<(u32, Response)>,
    write_task: JoinHandle<anyhow::Result<()>>,
}

impl<R: AsyncRead + Unpin> IpcServerConn<R> {
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
