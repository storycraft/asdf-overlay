use asdf_overlay_client::event::GpuLuid;
use neon::{
    prelude::{Context, FunctionContext},
    result::{JsResult, NeonResult},
    types::{JsPromise, JsUndefined},
};
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;
use windows::Win32::{
    Foundation::LUID,
    Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
};

pub fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(format!("{err:?}"))))
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

pub fn create_adapter_by_luid(luid: GpuLuid) -> anyhow::Result<Option<IDXGIAdapter>> {
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>()? };

    let luid = LUID {
        LowPart: luid.low,
        HighPart: luid.high,
    };
    let mut i = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters(i) } {
        i += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc() }) else {
            continue;
        };

        if desc.AdapterLuid == luid {
            return Ok(Some(adapter));
        }
    }

    Ok(None)
}
