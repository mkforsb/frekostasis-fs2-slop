//! Frekostasis DSP: an experimental drone machine built around a headless
//! Frekussion FK-2, the dual four-operator FM percussion synthesizer.
//!
//! A Euclidean clock strikes the FK-2 (both channels, MUTATE at RND, as if
//! Space were pressed). Each strike is rendered offline, a slice per audio
//! block, cut where its level dies away and kept in a small pool. From there
//! hits become *layers* of a PaulStretch-style spectral time stretcher,
//! which smears them tens to thousands of times longer into one continuous
//! drone. The drone goes through a ping-pong space echo and a Dattorro plate
//! with shimmer.
//!
//! No dependencies besides the FK-2's own DSP and no platform code, so it
//! compiles to a standalone wasm module for an AudioWorklet as well as
//! running natively.

#![allow(clippy::needless_range_loop)]

pub mod clock;
pub mod echo;
pub mod fft;
pub mod machine;
pub mod params;
pub mod plate;
pub mod presets;
pub mod source;
pub mod stretch;
pub mod telemetry;
pub mod util;

pub use machine::Machine;
pub use params::{ALL_PARAMS, PARAM_COUNT, Param, ParamKind, Patch};
