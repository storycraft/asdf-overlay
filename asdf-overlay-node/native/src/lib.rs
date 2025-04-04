use asdf_overlay_client::prelude::*;
use neon::{prelude::*, types::buffer::TypedArray};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use tokio::runtime::Runtime;

static CONN: Mutex<Option<IpcClientConn>> = Mutex::new(None);

fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(err.to_string())))
}

fn init(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let name = cx.argument::<JsString>(0)?.value(&mut cx);

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();

    rt.spawn(async move {
        *CONN.lock() = Some(
            inject(OwnedProcess::find_first_by_name(name).unwrap(), None)
                .await
                .unwrap(),
        );

        deferred.settle_with(&channel, move |mut cx| Ok(JsUndefined::new(&mut cx)));
    });

    Ok(promise)
}

fn update(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let width = cx.argument::<JsNumber>(0)?.value(&mut cx);
    let data = cx.argument::<JsBuffer>(1)?.as_slice(&mut cx).to_vec();

    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();

    rt.spawn(async move {
        if let Some(mut conn) = { CONN.lock().take() } {
            println!("sending update {} {}", width, data.len());
            conn.request(&Request::Texture(UpdateTexture {
                width: width as _,
                data,
            }))
            .await
            .unwrap();
            *CONN.lock() = Some(conn);
        }

        deferred.settle_with(&channel, move |mut cx| Ok(JsUndefined::new(&mut cx)));
    });

    Ok(promise)
}

fn close(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let rt = runtime(&mut cx)?;
    let channel = cx.channel();

    let (deferred, promise) = cx.promise();

    rt.spawn(async move {
        if let Some(mut conn) = { CONN.lock().take() } {
            conn.request(&Request::Close).await.unwrap();
        }

        deferred.settle_with(&channel, move |mut cx| Ok(JsUndefined::new(&mut cx)));
    });

    Ok(promise)
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("init", init)?;
    cx.export_function("update", update)?;
    cx.export_function("close", close)?;
    Ok(())
}
