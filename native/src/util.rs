use asdf_overlay_common::message::Request;
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

pub fn request_promise<'a>(cx: &mut FunctionContext<'a>, id: u32, request: Request) -> JsResult<'a, JsPromise> {
    let rt = runtime(cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();
    rt.spawn(async move {
        let res = MANAGER.request(id, &request).await;

        deferred.settle_with(&channel, move |mut cx| match res {
            Ok(_) => Ok(JsUndefined::new(&mut cx)),
            Err(err) => cx.throw_error(format!("{err:?}")),
        });
    });

    Ok(promise)
}
