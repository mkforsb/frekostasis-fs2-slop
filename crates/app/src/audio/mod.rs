//! Platform audio backends behind one small handle.
//!
//! * `web`: an AudioWorklet running the machine compiled to a standalone
//!   wasm module.
//! * `desktop`: a PulseAudio playback stream fed from a dedicated thread.
//!
//! Commands go down to the machine; telemetry (what the machine is doing:
//! the pattern position, the pool, the layers, the spectrum) comes back up
//! about thirty times a second into a signal the panel draws from.

// Without a platform feature only the null backend exists.
#![cfg_attr(not(any(feature = "web", feature = "desktop")), allow(dead_code))]

use dioxus::prelude::*;
use frekostasis_dsp::params::{ALL_PARAMS, Param, Patch};
use frekostasis_dsp::telemetry::{LEN as TELEMETRY_LEN, Telemetry};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;

#[cfg(feature = "desktop")]
mod pulse;
#[cfg(all(feature = "web", not(feature = "desktop")))]
mod web;

/// Blocks of 128 frames between telemetry reports (≈ 32 ms at 48 kHz).
/// `worklet.js` has its own copy.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub const TELEMETRY_BLOCKS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Param { param: Param, value: f32 },
    Play(bool),
    Strike,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioStatus {
    /// Web only: waiting for a user gesture before the AudioContext may start.
    #[cfg_attr(feature = "desktop", allow(dead_code))]
    NeedsGesture,
    Starting,
    Running {
        sample_rate: u32,
        detail: String,
    },
    Failed(String),
}

/// Last value sent for everything, so a backend that comes up late (the web
/// one, after a user gesture) can be brought in sync in one go.
#[derive(Clone, Debug)]
pub struct ParamCache {
    pub patch: Patch,
    pub playing: bool,
}

impl ParamCache {
    pub fn new(patch: Patch) -> Self {
        Self {
            patch,
            playing: false,
        }
    }

    #[cfg_attr(feature = "desktop", allow(dead_code))]
    pub fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Param { param, value } => self.patch[param.index()] = value,
            Command::Play(on) => self.playing = on,
            Command::Strike => {}
        }
    }

    /// Every parameter, then the transport.
    pub fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        ALL_PARAMS
            .iter()
            .map(|&param| Command::Param {
                param,
                value: self.patch[param.index()],
            })
            .chain(std::iter::once(Command::Play(self.playing)))
    }
}

pub type TelemetryTx = UnboundedSender<Box<Telemetry>>;

#[cfg(feature = "desktop")]
type Backend = pulse::PulseBackend;
#[cfg(all(feature = "web", not(feature = "desktop")))]
type Backend = web::WebBackend;
#[cfg(not(any(feature = "web", feature = "desktop")))]
type Backend = NullBackend;

/// Cheap to clone; shared through the Dioxus context.
#[derive(Clone)]
pub struct AudioHandle {
    backend: std::rc::Rc<Backend>,
    pub status: Signal<AudioStatus>,
    pub telemetry: Signal<Telemetry>,
}

impl AudioHandle {
    /// Must be called inside the Dioxus runtime (e.g. from `use_hook`).
    pub fn new(initial: ParamCache) -> Self {
        let status = Signal::new(AudioStatus::Starting);
        let telemetry = Signal::new([0.0; TELEMETRY_LEN]);
        let (tx, rx) = unbounded();
        spawn(receive_telemetry(rx, telemetry));
        let backend = std::rc::Rc::new(Backend::new(status, initial, tx));
        Self {
            backend,
            status,
            telemetry,
        }
    }

    pub fn send(&self, cmd: Command) {
        self.backend.send(cmd);
    }

    /// Call from user-gesture handlers; lets the web backend start or resume.
    pub fn user_gesture(&self) {
        self.backend.user_gesture();
    }
}

async fn receive_telemetry(
    mut rx: UnboundedReceiver<Box<Telemetry>>,
    mut signal: Signal<Telemetry>,
) {
    while let Some(mut t) = rx.next().await {
        // Only the newest report matters.
        while let Ok(newer) = rx.try_recv() {
            t = newer;
        }
        signal.set(*t);
    }
}

/// Used when building without a platform feature (e.g. `cargo check`).
#[cfg(not(any(feature = "web", feature = "desktop")))]
pub struct NullBackend;

#[cfg(not(any(feature = "web", feature = "desktop")))]
impl NullBackend {
    fn new(mut status: Signal<AudioStatus>, _initial: ParamCache, _tx: TelemetryTx) -> Self {
        status.set(AudioStatus::Failed("built without an audio backend".into()));
        Self
    }
    fn send(&self, _cmd: Command) {}
    fn user_gesture(&self) {}
}
