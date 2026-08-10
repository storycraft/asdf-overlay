pub mod ime;
pub mod input;

use anyhow::Context;
use asdf_overlay_client::client::IpcClientEventStream;
use asdf_overlay_common::event::{OverlayEvent, window::WindowEvent};
use napi::{
    bindgen_prelude::{FnArgs, Function, JsObjectValue, JsValuesTupleIntoVec, Object},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode, UnknownReturnValue},
};

use crate::event::input::InputEvent;

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
    struct Emitter(EmitTsFn);
    impl Emitter {
        fn emit<Args>(&self, args: Args)
        where
            FnArgs<Args>: JsValuesTupleIntoVec,
            Args: 'static,
        {
            self.0
                .call(VarArgs::of(args), ThreadsafeFunctionCallMode::Blocking);
        }
    }

    let emitter = Emitter(emit_tsfn);
    while let Some(event) = stream.recv().await {
        match event {
            OverlayEvent::Window { id, event } => match event {
                WindowEvent::Added { width, height } => {
                    emitter.emit(("added", id, width, height));
                }

                WindowEvent::Resized { width, height } => {
                    emitter.emit(("resized", id, width, height));
                }
                WindowEvent::Input(input) => match InputEvent::from(input) {
                    InputEvent::Cursor { event } => {
                        emitter.emit(("cursor_input", id, event));
                    }
                    InputEvent::Keyboard { event } => {
                        emitter.emit(("keyboard_input", id, event));
                    }
                },

                WindowEvent::Destroyed => {
                    emitter.emit(("destroyed", id));
                }
            },

            OverlayEvent::InputBlockingEnded => {
                emitter.emit(("input_blocking_ended",));
            }

            _ => {
                // TODO
            }
        }
    }

    emitter.emit(("disconnected",));
}
