pub mod ime;
pub mod input;

use anyhow::Context;
use asdf_overlay_client::client::IpcClientEventStream;
use asdf_overlay_event::{OverlayEvent, WindowEvent};
use napi::{
    bindgen_prelude::{FnArgs, Function, JsObjectValue, JsValuesTupleIntoVec, Object},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode, UnknownReturnValue},
};

use crate::{GpuLuid, event::input::InputEvent};

pub(crate) struct VarArgs(
    Box<dyn FnOnce(napi::sys::napi_env) -> napi::Result<Vec<napi::sys::napi_value>>>,
);

impl VarArgs {
    fn of<T>(args: T) -> Self
    where
        T: 'static,
        FnArgs<T>: JsValuesTupleIntoVec,
    {
        Self(Box::new(|env| FnArgs::from(args).into_vec(env)))
    }
}

impl JsValuesTupleIntoVec for VarArgs {
    fn into_vec(self, env: napi::sys::napi_env) -> napi::Result<Vec<napi::sys::napi_value>> {
        self.0(env)
    }
}

pub(crate) type EmitTsFn =
    ThreadsafeFunction<VarArgs, UnknownReturnValue, VarArgs, napi::Status, false>;

pub(crate) fn create_emit_tsfn<'env>(emitter: &Object<'env>) -> anyhow::Result<EmitTsFn> {
    let emit_fn = emitter
        .get_named_property::<Function<VarArgs, UnknownReturnValue>>("emit")
        .context("cannot find emit function of EventEmitter")?;
    let emit_fn = emit_fn.bind(emitter)?;

    emit_fn
        .build_threadsafe_function()
        .build()
        .context("failed to build threadsafe function")
}

pub(crate) async fn event_task(mut stream: IpcClientEventStream, emit_tsfn: EmitTsFn) {
    while let Some(event) = stream.recv().await {
        match event {
            OverlayEvent::Window { id, event } => match event {
                WindowEvent::Added {
                    width,
                    height,
                    gpu_id,
                } => {
                    emit_tsfn.call(
                        VarArgs::of(("added", id, width, height, GpuLuid::from(gpu_id))),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }

                WindowEvent::Resized { width, height } => {
                    emit_tsfn.call(
                        VarArgs::of(("resized", id, width, height)),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
                WindowEvent::Input(input) => match InputEvent::from(input) {
                    InputEvent::Cursor { event } => {
                        emit_tsfn.call(
                            VarArgs::of(("cursor_input", id, event)),
                            ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                    InputEvent::Keyboard { event } => {
                        emit_tsfn.call(
                            VarArgs::of(("keyboard_input", id, event)),
                            ThreadsafeFunctionCallMode::NonBlocking,
                        );
                    }
                },

                WindowEvent::InputBlockingEnded => {
                    emit_tsfn.call(
                        VarArgs::of(("input_blocking_ended", id)),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }

                WindowEvent::Destroyed => {
                    emit_tsfn.call(
                        VarArgs::of(("destroyed", id)),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
            },
        }
    }

    emit_tsfn.call(
        VarArgs::of(("disconnected",)),
        ThreadsafeFunctionCallMode::Blocking,
    );
}
