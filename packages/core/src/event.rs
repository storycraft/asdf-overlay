pub mod ime;
pub mod input;

use anyhow::Context;
use napi::{
    Env,
    bindgen_prelude::{FnArgs, Function, JsObjectValue, JsValuesTupleIntoVec, Object},
    threadsafe_function::{ThreadsafeCallContext, ThreadsafeFunction, UnknownReturnValue},
};

use crate::{GpuLuid, event::input::InputEvent};

pub struct VarArgs(Vec<napi::sys::napi_value>);

impl VarArgs {
    fn of<T>(env: &Env, args: T) -> napi::Result<Self>
    where
        FnArgs<T>: JsValuesTupleIntoVec,
    {
        Ok(Self(FnArgs::from(args).into_vec(env.raw())?))
    }
}

impl JsValuesTupleIntoVec for VarArgs {
    fn into_vec(self, _: napi::sys::napi_env) -> napi::Result<Vec<napi::sys::napi_value>> {
        Ok(self.0)
    }
}

pub type EmitTsFn = ThreadsafeFunction<OverlayEvent, UnknownReturnValue, VarArgs>;

pub fn create_emit_tsfn<'env>(emitter: &Object<'env>) -> anyhow::Result<EmitTsFn> {
    let emit_fn = emitter
        .get_named_property::<Function<VarArgs, UnknownReturnValue>>("emit")
        .context("cannot find emit function of EventEmitter")?;
    let emit_fn = emit_fn.bind(emitter)?;

    emit_fn
        .build_threadsafe_function::<OverlayEvent>()
        .callee_handled()
        .build_callback(emit_callback)
        .context("failed to build threadsafe function")
}

fn emit_callback(ctx: ThreadsafeCallContext<OverlayEvent>) -> napi::Result<VarArgs> {
    let env = ctx.env;

    Ok(match ctx.value {
        OverlayEvent::Window { id, event } => match event {
            WindowEvent::Added {
                width,
                height,
                gpu_id,
            } => VarArgs::of(&env, ("added", id, width, height, gpu_id))?,

            WindowEvent::Resized { width, height } => {
                VarArgs::of(&env, ("resized", id, width, height))?
            }
            WindowEvent::Input(event) => match event {
                InputEvent::Cursor { event } => VarArgs::of(&env, ("cursor_input", id, event))?,
                InputEvent::Keyboard { event } => VarArgs::of(&env, ("keyboard_input", id, event))?,
            },

            WindowEvent::InputBlockingEnded => VarArgs::of(&env, ("input_blocking_ended", id))?,

            WindowEvent::Destroyed => VarArgs::of(&env, ("destroyed", id))?,
        },

        OverlayEvent::Disconnected => VarArgs::of(&env, ("disconnected",))?,
    })
}

pub enum OverlayEvent {
    /// Events related to a specific window.
    Window {
        /// Unique identifier for the window.
        id: u32,
        event: WindowEvent,
    },

    Disconnected,
}

impl From<asdf_overlay_event::OverlayEvent> for OverlayEvent {
    fn from(event: asdf_overlay_event::OverlayEvent) -> Self {
        match event {
            asdf_overlay_event::OverlayEvent::Window { id, event } => Self::Window {
                id,
                event: event.into(),
            },
        }
    }
}

/// Describe a window event.
pub enum WindowEvent {
    /// A new window capable for overlay rendering is identified.
    Added {
        /// Initial width of the window.
        width: u32,

        /// Initial height of the window.
        height: u32,

        /// The LUID of the GPU adapter which the window used to present to surface.
        ///
        /// Client must choose correct GPU adapter using this luid,
        /// otherwise overlay rendering may fail.
        gpu_id: GpuLuid,
    },

    /// Window size is changed.
    Resized {
        /// New width of the window.
        width: u32,

        /// New height of the window.
        height: u32,
    },

    /// Input event related to this window.
    ///
    /// You only receive this event if you are listening to input events
    /// or have input blocking enabled for this window.
    Input(InputEvent),

    /// Input blocking is turned off or interrupted by the user or system.
    ///
    /// The user may turn off input blocking at any time,
    /// for example, by pressing Alt+F4 on Windows.
    InputBlockingEnded,

    /// Window is no longer available for overlay rendering.
    /// This is likely the last event for this window.
    Destroyed,
}

impl From<asdf_overlay_event::WindowEvent> for WindowEvent {
    fn from(event: asdf_overlay_event::WindowEvent) -> Self {
        match event {
            asdf_overlay_event::WindowEvent::Added {
                width,
                height,
                gpu_id,
            } => Self::Added {
                width,
                height,
                gpu_id: gpu_id.into(),
            },
            asdf_overlay_event::WindowEvent::Resized { width, height } => {
                Self::Resized { width, height }
            }
            asdf_overlay_event::WindowEvent::Input(input) => Self::Input(input.into()),
            asdf_overlay_event::WindowEvent::InputBlockingEnded => Self::InputBlockingEnded,
            asdf_overlay_event::WindowEvent::Destroyed => Self::Destroyed,
        }
    }
}
