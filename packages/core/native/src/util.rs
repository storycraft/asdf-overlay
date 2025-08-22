use crate::{MANAGER, runtime};
use asdf_overlay_client::client::IpcClientConn;
use neon::{
    prelude::{Context, FunctionContext},
    result::JsResult,
    types::{JsPromise, JsUndefined},
};

pub fn with_rt<'a>(
    cx: &mut FunctionContext<'a>,
    fut: impl Future<Output = anyhow::Result<()>> + Send + 'static,
) -> JsResult<'a, JsPromise> {
    let rt = runtime(cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = fut.await;
        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(_) => Ok(JsUndefined::new(&mut cx)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}

pub async fn try_with_ipc<T>(
    id: u32,
    f: impl AsyncFnOnce(&mut IpcClientConn) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    MANAGER
        .with(id, async move |overlay| {
            f(&mut *overlay.ipc.lock().await).await
        })
        .await?
}
