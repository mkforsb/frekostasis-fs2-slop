//! Factory voice presets and two-channel kits.
//!
//! Operator levels read like a DX panel: a carrier's level is its volume, a
//! modulator's is its modulation index. Ratios use [`R`], which picks the
//! RATIO knob position for a frequency ratio.

use core::f32::consts::PI;

use crate::VOICES;
use crate::params::{
    LFO_AMP, LFO_FM, LFO_PITCH, LFO_SAW, LFO_SH, LFO_SQR, LFO_TRI, NOISE_BP, NOISE_HP, Param,
    VoicePatch, XMOD_ALL, XMOD_CARRIERS, XMOD_MODULATORS, default_patch, ratio_knob,
};

pub struct Preset {
    pub name: &'static str,
    /// Grouping for the preset menu.
    pub category: &'static str,
    pub values: &'static [(Param, f32)],
}

impl Preset {
    /// Full patch: defaults overlaid with this preset's values.
    pub fn patch(&self) -> VoicePatch {
        let mut p = default_patch();
        for &(param, v) in self.values {
            p[param.index()] = param.sanitize(v);
        }
        p
    }
}

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// RATIO knob position for a frequency ratio.
#[allow(non_snake_case)]
const fn R(ratio: f32) -> f32 {
    ratio_knob(ratio)
}

const BP: f32 = NOISE_BP as f32;
const HP: f32 = NOISE_HP as f32;
const TRI: f32 = LFO_TRI as f32;
const SQR: f32 = LFO_SQR as f32;
const SAW: f32 = LFO_SAW as f32;
const SH: f32 = LFO_SH as f32;
const PITCH: f32 = LFO_PITCH as f32;
const FM: f32 = LFO_FM as f32;
const AMP: f32 = LFO_AMP as f32;
const CAR: f32 = XMOD_CARRIERS as f32;
const MOD: f32 = XMOD_MODULATORS as f32;
const ALL: f32 = XMOD_ALL as f32;
const ON: f32 = 1.0;

/// Algorithm switch positions (see `params::ALGORITHMS`).
const STACK: f32 = 0.0; // 4→3→2→1
const Y_STACK: f32 = 1.0; // (3+4)→2→1
const BRANCH: f32 = 2.0; // (2 + 4→3)→1
const FAN_IN: f32 = 3.0; // (2+3+4)→1
const PAIRS: f32 = 4.0; // 2→1 · 4→3
const FAN_OUT: f32 = 5.0; // 4→(1,2,3)
const PAIR_2: f32 = 6.0; // 2→1 · 3 · 4
const ADDITIVE: f32 = 7.0; // 1 · 2 · 3 · 4

use Param::*;

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Init",
        category: "Basic",
        values: &[],
    },
    // --- Kicks -----------------------------------------------------------
    Preset {
        name: "FM Kick",
        category: "Kick",
        values: &[
            (Tune, 0.15),
            (Decay, 0.47),
            (SweepAmount, 0.86),
            (SweepTime, 0.33),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.9),
            (Op1Decay, 0.7),
            (Op2Ratio, R(1.0)),
            (Op2Level, 0.5),
            (Op2Decay, 0.12),
            (VelFm, 0.6),
            (Drive, 0.25),
        ],
    },
    Preset {
        name: "Boom",
        category: "Kick",
        values: &[
            (Tune, 0.1),
            (Decay, 0.64),
            (SweepAmount, 0.76),
            (SweepTime, 0.46),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.95),
            (Op1Decay, 0.85),
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.22),
            (Op2Decay, 0.1),
            (Drive, 0.12),
            (Filter, 0.3),
            (Resonance, 0.1),
        ],
    },
    Preset {
        name: "Industrial Kick",
        category: "Kick",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.14),
            (Decay, 0.4),
            (SweepAmount, 0.92),
            (SweepTime, 0.3),
            (Feedback, 0.62),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.9),
            (Op1Decay, 0.62),
            (Op2Ratio, R(1.41)),
            (Op2Level, 0.55),
            (Op2Decay, 0.24),
            (Op3Ratio, R(3.5)),
            (Op3Level, 0.4),
            (Op3Decay, 0.16),
            (Op4Ratio, R(1.0)),
            (Op4Level, 0.45),
            (Op4Decay, 0.1),
            (Drive, 0.75),
            (Filter, 0.34),
            (Resonance, 0.3),
        ],
    },
    Preset {
        name: "Sub Drop",
        category: "Kick",
        values: &[
            (Tune, 0.05),
            (Decay, 0.76),
            (SweepAmount, 0.82),
            (SweepTime, 0.72),
            (Op1Level, 0.95),
            (Op1Decay, 0.9),
            (Op2Level, 0.12),
            (Op2Decay, 0.2),
        ],
    },
    // --- Snares, claps, rims --------------------------------------------
    Preset {
        name: "FM Snare",
        category: "Snare",
        values: &[
            (Algorithm, PAIRS),
            (Tune, 0.46),
            (Decay, 0.36),
            (SweepAmount, 0.7),
            (SweepTime, 0.3),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.75),
            (Op1Decay, 0.45),
            (Op2Ratio, R(1.59)),
            (Op2Level, 0.38),
            (Op2Decay, 0.22),
            (Op3Ratio, R(2.3)),
            (Op3Level, 0.45),
            (Op3Decay, 0.35),
            (Op4Ratio, R(1.41)),
            (Op4Level, 0.35),
            (Op4Decay, 0.3),
            (NoiseLevel, 0.65),
            (NoiseType, BP),
            (NoiseTone, 0.74),
            (NoiseRes, 0.2),
            (NoiseDecay, 0.44),
            (NoiseFm, 0.3),
            (ReverbMix, 0.3),
            (ReverbDecay, 0.25),
            (ReverbTone, 0.7),
        ],
    },
    Preset {
        name: "Tight Snare",
        category: "Snare",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.55),
            (Decay, 0.34),
            (SweepAmount, 0.66),
            (SweepTime, 0.2),
            (Op1Level, 0.7),
            (Op1Decay, 0.4),
            (Op2Ratio, R(2.14)),
            (Op2Level, 0.45),
            (Op2Decay, 0.18),
            (Op3Ratio, R(5.4)),
            (Op3Level, 0.3),
            (Op3Decay, 0.12),
            (NoiseLevel, 0.7),
            (NoiseType, HP),
            (NoiseTone, 0.68),
            (NoiseDecay, 0.5),
            (NoiseFm, 0.45),
            (Drive, 0.3),
        ],
    },
    Preset {
        name: "Rimshot",
        category: "Snare",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.66),
            (Decay, 0.2),
            (SweepAmount, 0.64),
            (SweepTime, 0.14),
            (Op1Level, 0.8),
            (Op1Decay, 0.5),
            (Op2Ratio, R(2.76)),
            (Op2Level, 0.55),
            (Op2Decay, 0.1),
            (Op3Ratio, R(5.4)),
            (Op3Level, 0.3),
            (Op3Decay, 0.08),
            (NoiseLevel, 0.45),
            (NoiseType, HP),
            (NoiseTone, 0.8),
            (NoiseDecay, 0.18),
            (Filter, 0.62),
            (Level, 0.85),
        ],
    },
    Preset {
        name: "Buzz Clap",
        category: "Snare",
        values: &[
            (Decay, 0.3),
            (Op1Level, 0.0),
            (Op2Level, 0.0),
            (NoiseLevel, 0.95),
            (NoiseType, BP),
            (NoiseTone, 0.64),
            (NoiseRes, 0.35),
            (NoiseDecay, 0.47),
            (LfoWave, SAW),
            (LfoDest, AMP),
            (LfoRate, 0.96),
            (LfoDepth, 0.9),
            (ReverbMix, 0.4),
            (ReverbDecay, 0.3),
            (ReverbPredelay, 0.15),
        ],
    },
    // --- Toms and hand drums ---------------------------------------------
    Preset {
        name: "FM Tom",
        category: "Tom",
        values: &[
            (Tune, 0.34),
            (Decay, 0.5),
            (SweepAmount, 0.72),
            (SweepTime, 0.48),
            (Op1Level, 0.85),
            (Op1Decay, 0.65),
            (Op2Ratio, R(1.59)),
            (Op2Level, 0.3),
            (Op2Decay, 0.22),
            (ReverbMix, 0.35),
            (ReverbDecay, 0.35),
        ],
    },
    Preset {
        name: "Bwow",
        category: "Tom",
        values: &[
            (Tune, 0.42),
            (Decay, 0.5),
            (SweepAmount, 0.28),
            (SweepTime, 0.56),
            (Op1Level, 0.85),
            (Op1Decay, 0.7),
            (Op2Ratio, R(1.0)),
            (Op2Level, 0.36),
            (Op2Decay, 0.5),
            (VelFm, 0.8),
        ],
    },
    Preset {
        name: "FM Conga",
        category: "Tom",
        values: &[
            (Tune, 0.5),
            (Decay, 0.42),
            (SweepAmount, 0.6),
            (SweepTime, 0.25),
            (Op1Level, 0.85),
            (Op1Decay, 0.55),
            (Op2Ratio, R(1.5)),
            (Op2Level, 0.34),
            (Op2Decay, 0.16),
            (NoiseLevel, 0.3),
            (NoiseType, BP),
            (NoiseTone, 0.55),
            (NoiseDecay, 0.12),
        ],
    },
    Preset {
        name: "Wobble Tom",
        category: "Tom",
        values: &[
            (Tune, 0.3),
            (Decay, 0.58),
            (SweepAmount, 0.74),
            (SweepTime, 0.4),
            (Op2Ratio, R(1.5)),
            (Op2Level, 0.4),
            (Op2Decay, 0.35),
            (LfoWave, SQR),
            (LfoDest, PITCH),
            (LfoRate, 0.72),
            (LfoDepth, 0.3),
        ],
    },
    // --- Hats, cymbals, shakers ------------------------------------------
    Preset {
        name: "Closed Hat",
        category: "Cymbal",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.72),
            (Decay, 0.3),
            (Feedback, 0.5),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.75),
            (Op1Decay, 0.62),
            (Op2Ratio, R(13.34)),
            (Op2Level, 0.8),
            (Op2Decay, 0.5),
            (Op3Ratio, R(7.07)),
            (Op3Level, 0.7),
            (Op3Decay, 0.5),
            (Op4Ratio, R(2.76)),
            (Op4Level, 0.6),
            (Op4Decay, 0.5),
            (NoiseLevel, 0.5),
            (NoiseType, HP),
            (NoiseTone, 0.88),
            (NoiseDecay, 0.26),
            (Filter, 0.84),
            (Resonance, 0.15),
        ],
    },
    Preset {
        name: "Open Hat",
        category: "Cymbal",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.72),
            (Decay, 0.58),
            (Feedback, 0.5),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.75),
            (Op1Decay, 0.8),
            (Op2Ratio, R(13.34)),
            (Op2Level, 0.8),
            (Op2Decay, 0.75),
            (Op3Ratio, R(7.07)),
            (Op3Level, 0.7),
            (Op3Decay, 0.75),
            (Op4Ratio, R(2.76)),
            (Op4Level, 0.6),
            (Op4Decay, 0.75),
            (NoiseLevel, 0.45),
            (NoiseType, HP),
            (NoiseTone, 0.88),
            (NoiseDecay, 0.62),
            (Filter, 0.84),
            (Resonance, 0.15),
        ],
    },
    Preset {
        name: "Ride",
        category: "Cymbal",
        values: &[
            (Algorithm, FAN_OUT),
            (Tune, 0.7),
            (Decay, 0.72),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.6),
            (Op1Decay, 0.8),
            (Op2Ratio, R(1.41)),
            (Op2Level, 0.5),
            (Op2Decay, 0.75),
            (Op3Ratio, R(2.76)),
            (Op3Level, 0.45),
            (Op3Decay, 0.7),
            (Op4Ratio, R(5.4)),
            (Op4Level, 0.55),
            (Op4Decay, 0.72),
            (Feedback, 0.35),
            (NoiseLevel, 0.25),
            (NoiseType, HP),
            (NoiseTone, 0.9),
            (NoiseDecay, 0.55),
            (Filter, 0.74),
            (ReverbMix, 0.3),
            (ReverbDecay, 0.5),
        ],
    },
    Preset {
        name: "Crash",
        category: "Cymbal",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.76),
            (Decay, 0.74),
            (Feedback, 0.82),
            (Op1Level, 0.7),
            (Op1Decay, 0.78),
            (Op2Ratio, R(8.93)),
            (Op2Level, 0.82),
            (Op2Decay, 0.78),
            (Op3Ratio, R(PI)),
            (Op3Level, 0.7),
            (Op3Decay, 0.75),
            (Op4Ratio, R(1.41)),
            (Op4Level, 0.7),
            (Op4Decay, 0.75),
            (NoiseLevel, 0.55),
            (NoiseType, HP),
            (NoiseTone, 0.82),
            (NoiseDecay, 0.7),
            (Filter, 0.86),
            (ReverbMix, 0.45),
            (ReverbDecay, 0.6),
        ],
    },
    Preset {
        name: "Shaker",
        category: "Cymbal",
        values: &[
            (Decay, 0.3),
            (Op1Level, 0.0),
            (Op2Level, 0.0),
            (NoiseLevel, 0.7),
            (NoiseType, HP),
            (NoiseTone, 0.9),
            (NoiseRes, 0.3),
            (NoiseDecay, 0.42),
            (Sense, 0.6),
        ],
    },
    // --- Metal and bells -------------------------------------------------
    Preset {
        name: "Cowbell",
        category: "Metal",
        values: &[
            (Algorithm, PAIRS),
            (Tune, 0.69),
            (Decay, 0.45),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.75),
            (Op1Decay, 0.6),
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.38),
            (Op2Decay, 0.5),
            (Op3Ratio, R(1.5)),
            (Op3Level, 0.75),
            (Op3Decay, 0.6),
            (Op4Ratio, R(3.0)),
            (Op4Level, 0.38),
            (Op4Decay, 0.5),
            (Filter, 0.66),
            (Resonance, 0.4),
        ],
    },
    Preset {
        name: "Anvil",
        category: "Metal",
        values: &[
            (Algorithm, FAN_IN),
            (Tune, 0.62),
            (Decay, 0.6),
            (Op1Level, 0.85),
            (Op1Decay, 0.75),
            (Op2Ratio, R(2.76)),
            (Op2Level, 0.45),
            (Op2Decay, 0.6),
            (Op3Ratio, R(5.4)),
            (Op3Level, 0.35),
            (Op3Decay, 0.45),
            (Op4Ratio, R(8.93)),
            (Op4Level, 0.32),
            (Op4Decay, 0.3),
            (ReverbMix, 0.3),
            (ReverbDecay, 0.45),
        ],
    },
    Preset {
        name: "Clang",
        category: "Metal",
        values: &[
            (Algorithm, STACK),
            (Tune, 0.55),
            (Decay, 0.55),
            (Feedback, 0.3),
            (Op1Level, 0.8),
            (Op1Decay, 0.72),
            (Op2Ratio, R(1.41)),
            (Op2Level, 0.55),
            (Op2Decay, 0.62),
            (Op3Ratio, R(PI)),
            (Op3Level, 0.45),
            (Op3Decay, 0.5),
            (Op4Ratio, R(0.75)),
            (Op4Level, 0.35),
            (Op4Decay, 0.35),
            (Drive, 0.2),
        ],
    },
    Preset {
        name: "Metal Pipe",
        category: "Metal",
        values: &[
            (Algorithm, ADDITIVE),
            (Tune, 0.58),
            (Decay, 0.7),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.8),
            (Op2Ratio, R(2.76)),
            (Op2Level, 0.65),
            (Op2Decay, 0.66),
            (Op3Ratio, R(5.4)),
            (Op3Level, 0.5),
            (Op3Decay, 0.52),
            (Op4Ratio, R(8.93)),
            (Op4Level, 0.4),
            (Op4Decay, 0.38),
            (NoiseLevel, 0.2),
            (NoiseType, BP),
            (NoiseTone, 0.8),
            (NoiseDecay, 0.08),
            (ReverbMix, 0.25),
            (ReverbDecay, 0.4),
        ],
    },
    Preset {
        name: "Brake Drum",
        category: "Metal",
        values: &[
            (Algorithm, PAIRS),
            (Tune, 0.6),
            (Decay, 0.5),
            (Op1Level, 0.8),
            (Op1Decay, 0.62),
            (Op2Ratio, R(PI)),
            (Op2Level, 0.5),
            (Op2Decay, 0.4),
            (Op3Ratio, R(2.14)),
            (Op3Level, 0.7),
            (Op3Decay, 0.58),
            (Op4Ratio, R(5.4)),
            (Op4Level, 0.45),
            (Op4Decay, 0.35),
            (Drive, 0.35),
        ],
    },
    Preset {
        name: "Tubular Bell",
        category: "Metal",
        values: &[
            (Algorithm, PAIRS),
            (Tune, 0.62),
            (Decay, 0.82),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.85),
            (Op2Ratio, R(3.5)),
            (Op2Level, 0.42),
            (Op2Decay, 0.7),
            (Op3Ratio, R(2.0)),
            (Op3Level, 0.5),
            (Op3Decay, 0.75),
            (Op4Ratio, R(7.07)),
            (Op4Level, 0.3),
            (Op4Decay, 0.45),
            (VelFm, 0.7),
            (ReverbMix, 0.45),
            (ReverbDecay, 0.65),
        ],
    },
    Preset {
        name: "Gong",
        category: "Metal",
        values: &[
            (Algorithm, Y_STACK),
            (Tune, 0.3),
            (Decay, 0.86),
            (SweepAmount, 0.46),
            (SweepTime, 0.8),
            (Feedback, 0.4),
            (Op1Level, 0.85),
            (Op1Decay, 0.9),
            (Op2Ratio, R(1.41)),
            (Op2Level, 0.5),
            (Op2Decay, 0.85),
            (Op3Ratio, R(2.76)),
            (Op3Level, 0.32),
            (Op3Decay, 0.8),
            (Op4Ratio, R(0.5)),
            (Op4Level, 0.3),
            (Op4Decay, 0.75),
            (LfoWave, TRI),
            (LfoDest, FM),
            (LfoRate, 0.35),
            (LfoDepth, 0.4),
            (ReverbMix, 0.55),
            (ReverbDecay, 0.8),
            (ReverbTone, 0.45),
        ],
    },
    // --- Tuned wood and pans ---------------------------------------------
    Preset {
        name: "Marimba",
        category: "Tuned",
        values: &[
            (Algorithm, PAIR_2),
            (Tune, 0.6),
            (Decay, 0.55),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.62),
            (Op2Ratio, R(4.0)),
            (Op2Level, 0.3),
            (Op2Decay, 0.18),
            (Op3Ratio, R(4.0)),
            (Op3Level, 0.35),
            (Op3Decay, 0.4),
            (Op4Ratio, R(10.0)),
            (Op4Level, 0.2),
            (Op4Decay, 0.2),
            (ReverbMix, 0.3),
            (ReverbDecay, 0.4),
            (Level, 0.9),
        ],
    },
    Preset {
        name: "Steel Drum",
        category: "Tuned",
        values: &[
            (Algorithm, PAIR_2),
            (Tune, 0.62),
            (Decay, 0.58),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.66),
            (Op2Ratio, R(1.0)),
            (Op2Level, 0.34),
            (Op2Decay, 0.3),
            (Op3Ratio, R(2.0)),
            (Op3Level, 0.5),
            (Op3Decay, 0.55),
            (Op4Ratio, R(3.0)),
            (Op4Level, 0.3),
            (Op4Decay, 0.4),
            (SweepAmount, 0.44),
            (SweepTime, 0.3),
            (ReverbMix, 0.35),
            (ReverbDecay, 0.45),
        ],
    },
    Preset {
        name: "Clave",
        category: "Tuned",
        values: &[
            (Tune, 0.97),
            (Decay, 0.22),
            (Op1Level, 0.85),
            (Op1Decay, 0.55),
            (Op2Ratio, R(2.76)),
            (Op2Level, 0.2),
            (Op2Decay, 0.1),
        ],
    },
    Preset {
        name: "Log Drum",
        category: "Tuned",
        values: &[
            (Algorithm, PAIR_2),
            (Tune, 0.46),
            (Decay, 0.5),
            (SweepAmount, 0.58),
            (SweepTime, 0.15),
            (Op1Level, 0.85),
            (Op1Decay, 0.55),
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.3),
            (Op2Decay, 0.12),
            (Op3Ratio, R(2.76)),
            (Op3Level, 0.3),
            (Op3Decay, 0.25),
            (Op4Level, 0.0),
            (Level, 0.9),
        ],
    },
    // --- Hollow, discordant, stabs ---------------------------------------
    Preset {
        name: "Hollow Hit",
        category: "Hollow",
        values: &[
            (Algorithm, PAIR_2),
            (Tune, 0.45),
            (Decay, 0.4),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.85),
            (Op1Decay, 0.6),
            // A modulator an octave above a carrier gives only odd harmonics.
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.45),
            (Op2Decay, 0.4),
            (Op3Ratio, R(0.5)),
            (Op3Level, 0.4),
            (Op3Decay, 0.5),
            (Op4Level, 0.0),
            (Filter, 0.36),
            (Resonance, 0.55),
            (ReverbMix, 0.3),
            (ReverbDecay, 0.4),
        ],
    },
    Preset {
        name: "Bottle",
        category: "Hollow",
        values: &[
            (Tune, 0.66),
            (Decay, 0.44),
            (SweepAmount, 0.4),
            (SweepTime, 0.4),
            (Op1Level, 0.85),
            (Op1Decay, 0.55),
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.26),
            (Op2Decay, 0.35),
            (NoiseLevel, 0.35),
            (NoiseType, BP),
            (NoiseTone, 0.7),
            (NoiseRes, 0.6),
            (NoiseDecay, 0.2),
            (NoiseFm, 0.15),
        ],
    },
    Preset {
        name: "Hollow Stab",
        category: "Hollow",
        values: &[
            (Algorithm, PAIRS),
            (Tune, 0.5),
            (Decay, 0.32),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.5),
            (Op2Ratio, R(2.0)),
            (Op2Level, 0.55),
            (Op2Decay, 0.3),
            (Op3Ratio, R(1.5)),
            (Op3Level, 0.7),
            (Op3Decay, 0.5),
            (Op4Ratio, R(3.0)),
            (Op4Level, 0.5),
            (Op4Decay, 0.3),
            (Drive, 0.3),
            (Filter, 0.32),
            (Resonance, 0.6),
            (ReverbMix, 0.4),
            (ReverbDecay, 0.45),
            (ReverbPredelay, 0.2),
        ],
    },
    Preset {
        name: "Discord Stab",
        category: "Hollow",
        values: &[
            (Algorithm, FAN_OUT),
            (Tune, 0.5),
            (Decay, 0.42),
            (Feedback, 0.45),
            (Op1Ratio, R(1.0)),
            (Op1Level, 0.8),
            (Op1Decay, 0.6),
            (Op2Ratio, R(1.41)),
            (Op2Level, 0.7),
            (Op2Decay, 0.6),
            (Op3Ratio, R(1.73)),
            (Op3Level, 0.65),
            (Op3Decay, 0.6),
            (Op4Ratio, R(0.5)),
            (Op4Level, 0.4),
            (Op4Decay, 0.35),
            (Drive, 0.45),
            (ReverbMix, 0.4),
            (ReverbDecay, 0.5),
        ],
    },
    // --- Effects ---------------------------------------------------------
    Preset {
        name: "Laser Zap",
        category: "FX",
        values: &[
            (Tune, 0.5),
            (Decay, 0.44),
            (SweepAmount, 1.0),
            (SweepTime, 0.56),
            (Op1Level, 0.85),
            (Op1Decay, 0.7),
            (Op2Ratio, R(1.0)),
            (Op2Level, 0.3),
            (Op2Decay, 0.55),
            (ReverbMix, 0.4),
            (ReverbDecay, 0.55),
            (ReverbPredelay, 0.35),
        ],
    },
    Preset {
        name: "Glitch Burst",
        category: "FX",
        values: &[
            (Algorithm, BRANCH),
            (Tune, 0.62),
            (Decay, 0.52),
            (Feedback, 0.7),
            (Op1Level, 0.8),
            (Op1Decay, 0.7),
            (Op2Ratio, R(PI)),
            (Op2Level, 0.4),
            (Op2Decay, 0.62),
            (Op3Ratio, R(1.5)),
            (Op3Level, 0.5),
            (Op3Decay, 0.65),
            (Op4Ratio, R(5.0)),
            (Op4Level, 0.5),
            (Op4Decay, 0.62),
            (LfoWave, SH),
            (LfoDest, PITCH),
            (LfoRate, 0.82),
            (LfoDepth, 0.6),
            (Drive, 0.4),
        ],
    },
    Preset {
        name: "FM Growl",
        category: "FX",
        values: &[
            (Tune, 0.28),
            (Decay, 0.52),
            (SweepAmount, 0.7),
            (SweepTime, 0.45),
            (Op1Level, 0.85),
            (Op1Decay, 0.75),
            (Op2Ratio, R(1.5)),
            (Op2Level, 0.55),
            (Op2Decay, 0.7),
            (LfoWave, TRI),
            (LfoDest, FM),
            (LfoRate, 0.62),
            (LfoDepth, 0.9),
            (Drive, 0.5),
            (Filter, 0.3),
            (Resonance, 0.45),
        ],
    },
    Preset {
        name: "Noise Snap",
        category: "FX",
        values: &[
            (Decay, 0.2),
            (Op1Level, 0.0),
            (Op2Level, 0.0),
            (NoiseLevel, 0.9),
            (NoiseType, BP),
            (NoiseTone, 0.62),
            (NoiseRes, 0.15),
            (NoiseDecay, 0.42),
        ],
    },
];

/// A two-channel setup, typically using the cross-channel controls.
pub struct Kit {
    pub name: &'static str,
    /// Per channel: a preset name, then values that override it.
    pub channels: [(&'static str, &'static [(Param, f32)]); VOICES],
}

impl Kit {
    pub fn patches(&self) -> [VoicePatch; VOICES] {
        self.channels.map(|(name, overrides)| {
            let mut p = preset(name).map_or_else(default_patch, Preset::patch);
            for &(param, v) in overrides {
                p[param.index()] = param.sanitize(v);
            }
            p
        })
    }
}

pub const KITS: &[Kit] = &[
    Kit {
        name: "Kick & Snare",
        channels: [("FM Kick", &[(Pan, 0.45)]), ("FM Snare", &[(Pan, 0.55)])],
    },
    Kit {
        name: "Kick & Hat",
        channels: [("Boom", &[]), ("Closed Hat", &[(Pan, 0.6)])],
    },
    Kit {
        // Channel 2 is a noise snap fired with every hit on channel 1, and
        // the tom's body phase-modulates it: a snare with a tuned body.
        name: "Body Snare",
        channels: [
            ("FM Tom", &[(Tune, 0.4), (Decay, 0.38), (ReverbMix, 0.25)]),
            (
                "Noise Snap",
                &[
                    (Link, ON),
                    (Op1Level, 0.6),
                    (Op1Ratio, R(1.0)),
                    (Tune, 0.6),
                    (XmodAmount, 0.6),
                    (XmodDest, CAR),
                ],
            ),
        ],
    },
    Kit {
        // Each channel modulates the other: inharmonic, unstable clangs.
        name: "Cross Clang",
        channels: [
            (
                "Metal Pipe",
                &[(XmodAmount, 0.45), (XmodDest, ALL), (Pan, 0.35)],
            ),
            (
                "Anvil",
                &[(Link, ON), (Tune, 0.67), (XmodAmount, 0.5), (Pan, 0.65)],
            ),
        ],
    },
    Kit {
        name: "Ring Bells",
        channels: [
            ("Tubular Bell", &[(Pan, 0.4)]),
            (
                "Cowbell",
                &[
                    (Link, ON),
                    (Ring, 0.85),
                    (Decay, 0.6),
                    (Tune, 0.74),
                    (Pan, 0.6),
                ],
            ),
        ],
    },
    Kit {
        name: "Chaos Pair",
        channels: [
            (
                "Discord Stab",
                &[
                    (XmodAmount, 0.7),
                    (XmodDest, MOD),
                    (Pan, 0.3),
                    (Level, 0.65),
                ],
            ),
            (
                "Hollow Stab",
                &[
                    (Link, ON),
                    (Tune, 0.57),
                    (XmodAmount, 0.65),
                    (XmodDest, ALL),
                    (Pan, 0.7),
                    (Level, 0.62),
                ],
            ),
        ],
    },
    Kit {
        name: "Gamelan",
        channels: [
            ("Gong", &[(Tune, 0.36), (Pan, 0.4)]),
            (
                "Marimba",
                &[(Tune, 0.66), (XmodAmount, 0.25), (Ring, 0.3), (Pan, 0.6)],
            ),
        ],
    },
    Kit {
        name: "Industrial",
        channels: [
            ("Industrial Kick", &[]),
            (
                "Brake Drum",
                &[(XmodAmount, 0.5), (XmodDest, MOD), (ReverbMix, 0.35)],
            ),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_unique() {
        for (i, a) in PRESETS.iter().enumerate() {
            for b in &PRESETS[i + 1..] {
                assert_ne!(a.name, b.name);
            }
        }
    }

    #[test]
    fn presets_are_in_range() {
        for p in PRESETS {
            for &(param, v) in p.values {
                assert_eq!(param.sanitize(v), v, "{}: {:?}", p.name, param);
            }
        }
    }

    #[test]
    fn kits_use_existing_presets() {
        for k in KITS {
            for (name, overrides) in k.channels {
                assert!(preset(name).is_some(), "{}: no preset {name}", k.name);
                for &(param, v) in overrides {
                    assert_eq!(param.sanitize(v), v, "{}: {:?}", k.name, param);
                }
            }
        }
    }

    #[test]
    fn ratio_helper_hits_table_entries() {
        assert_eq!(crate::params::map::op_ratio(R(2.76)), 2.76);
        assert_eq!(crate::params::map::op_ratio(R(13.34)), 13.34);
    }
}
