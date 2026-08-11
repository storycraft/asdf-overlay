pub mod ime;
pub mod input;
pub mod tracing;

use anyhow::Context;
use asdf_overlay_client::client::IpcClientEventStream;
use asdf_overlay_client::common::event::surface::SurfaceEvent;
use asdf_overlay_client::common::event::tracing::TracingEvent;
use asdf_overlay_client::common::event::{OverlayEvent, window::WindowEvent};
use napi::{
    bindgen_prelude::{FnArgs, Function, JsObjectValue, JsValuesTupleIntoVec, Object},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode, UnknownReturnValue},
};

use crate::GpuLuid;
use crate::event::input::InputEvent;
use crate::event::tracing::TracingMetadata;

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
                    emitter.emit(("window_added", id, width, height));
                }

                WindowEvent::Resized { width, height } => {
                    emitter.emit(("window_resized", id, width, height));
                }
                WindowEvent::Input(input) => match InputEvent::from(input) {
                    InputEvent::Cursor { event } => {
                        emitter.emit(("window_cursor_input", id, event));
                    }
                    InputEvent::Keyboard { event } => {
                        emitter.emit(("window_keyboard_input", id, event));
                    }
                },

                WindowEvent::Destroyed => {
                    emitter.emit(("window_destroyed", id));
                }
            },

            OverlayEvent::InputBlockingEnded => {
                emitter.emit(("input_blocking_ended",));
            }

            OverlayEvent::Surface { id, event } => match event {
                SurfaceEvent::Added {
                    width,
                    height,
                    gpu_id,
                } => {
                    emitter.emit(("surface_added", id, width, height, GpuLuid::from(gpu_id)));
                }
                SurfaceEvent::Destroyed => {
                    emitter.emit(("surface_destroyed", id));
                }
            },

            OverlayEvent::Tracing(event) => match event {
                TracingEvent::Enter(metadata) => {
                    emitter.emit(("tracing_enter", TracingMetadata::from(metadata)));
                }
                TracingEvent::Event { metadata, message } => {
                    emitter.emit(("tracing_event", TracingMetadata::from(metadata), message));
                }
                TracingEvent::Exit => {
                    emitter.emit(("tracing_exit",));
                }
            },
        }
    }

    emitter.emit(("disconnected",));
}
