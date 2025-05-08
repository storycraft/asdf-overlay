use asdf_overlay_client::common::ipc::server::IpcServerConn;
use neon::{
    prelude::{Context, FunctionContext},
    result::JsResult,
    types::{JsPromise, JsUndefined},
};
use windows::Win32::{
    Foundation::HANDLE,
    System::{
        SystemInformation::{IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_UNKNOWN},
        Threading::IsWow64Process2,
    },
};

use crate::{MANAGER, runtime};

pub fn get_process_arch(handle: HANDLE) -> IMAGE_FILE_MACHINE {
    let mut native_output = IMAGE_FILE_MACHINE_UNKNOWN;
    let mut wow64_output = IMAGE_FILE_MACHINE_UNKNOWN;
    unsafe {
        _ = IsWow64Process2(handle, &mut wow64_output, Some(&mut native_output));
    }

    if wow64_output != IMAGE_FILE_MACHINE_UNKNOWN {
        wow64_output
    } else {
        native_output
    }
}

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
    f: impl AsyncFnOnce(&mut IpcServerConn) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    MANAGER
        .with(id, async move |overlay| {
            f(&mut *overlay.ipc.lock().await).await
        })
        .await?
}
