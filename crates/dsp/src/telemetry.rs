//! What the machine reports to the panel, as a flat `f32` array (so the
//! worklet can post it as one `Float32Array`).

use crate::source::OUTLINE;
use crate::stretch::{MAX_LAYERS, SPECTRUM_BANDS};

/// 1 while playing (or releasing after STOP).
pub const PLAYING: usize = 0;
pub const FROZEN: usize = 1;
/// Current pattern step and progress through it.
pub const STEP: usize = 2;
pub const STEP_PHASE: usize = 3;
pub const STEPS: usize = 4;
/// Pattern pulses as a bit mask.
pub const MASK: usize = 5;
/// Strikes so far (for flashing the strike LED).
pub const STRIKES: usize = 6;
/// Hits waiting, how many of them are up to date, and the planned depth.
pub const POOL: usize = 7;
pub const POOL_FRESH: usize = 8;
pub const POOL_TARGET: usize = 9;
/// Progress of the hit being rendered (0 when idle).
pub const RENDERING: usize = 10;
/// Average length of rendered hits, seconds.
pub const HIT_AVERAGE: usize = 11;
/// Strikes that found the pool empty and re-used the last hit.
pub const STARVED: usize = 12;
pub const LAYERS: usize = 13;
/// Output peak levels.
pub const PEAK_L: usize = 14;
pub const PEAK_R: usize = 15;
/// The most recent strike: length, the two channels' algorithms and pitches.
pub const HIT_SECONDS: usize = 16;
pub const HIT_ALGO_A: usize = 17;
pub const HIT_ALGO_B: usize = 18;
pub const HIT_PITCH_A: usize = 19;
pub const HIT_PITCH_B: usize = 20;
/// Stretcher hop, seconds.
pub const HOP_SECONDS: usize = 21;
/// Frames the stretcher had to finish in one go (should stay 0).
pub const OVERRUNS: usize = 22;
/// Output gain of the stretched stream (falls to 0 after STOP).
pub const PLAY_GAIN: usize = 23;
/// Per layer: progress through its source, gain, algorithm A, algorithm B.
pub const LAYER_BASE: usize = 24;
pub const LAYER_FIELDS: usize = 4;
pub const SPECTRUM_BASE: usize = LAYER_BASE + MAX_LAYERS * LAYER_FIELDS;
/// Min/max outline of the most recent strike.
pub const OUTLINE_BASE: usize = SPECTRUM_BASE + SPECTRUM_BANDS;
pub const LEN: usize = OUTLINE_BASE + 2 * OUTLINE;

pub type Telemetry = [f32; LEN];
