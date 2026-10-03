//! Frekussion DSP: a dual four-operator FM percussion synthesizer, in the
//! spirit of the Syncussion-style two-channel drum synths, with cross
//! modulation between the channels.
//!
//! The crate has no dependencies and no platform code so it can be compiled to
//! a standalone wasm module for an AudioWorklet as well as used natively.

pub mod engine;
pub mod params;
pub mod presets;
pub mod reverb;
pub mod voice;

pub use engine::{Engine, VOICES};
pub use params::{ALL_PARAMS, OPS, PARAM_COUNT, Param, ParamKind, VoicePatch};
