//! Factory scenes. Each overrides some controls; everything else (and the
//! master level, which scenes never touch) keeps its default.

use crate::params::{Param, Patch, default_patch, origin_index};

pub struct Scene {
    pub name: &'static str,
    pub values: &'static [(Param, f32)],
    /// FK-2 kit to start from, by name.
    pub origin: Option<&'static str>,
}

impl Scene {
    /// The scene on top of the defaults, keeping `current`'s master level.
    pub fn patch(&self, current: &Patch) -> Patch {
        let mut p = default_patch();
        for &(param, v) in self.values {
            p[param.index()] = param.sanitize(v);
        }
        if let Some(name) = self.origin {
            p[Param::Origin.index()] = origin_index(name) as f32;
        }
        p[Param::Master.index()] = current[Param::Master.index()];
        p
    }
}

// FK-2 algorithm bits for the ALGORITHMS mask (see `frekussion_dsp::params::ALGORITHMS`).
/// 4→3→2→1, (3+4)→2→1, (2 + 4→3)→1, (2+3+4)→1: deep FM, noisy and metallic.
const STACKS: f32 = 15.0;
/// 2→1 · 4→3: two independent pairs.
const PAIRS: f32 = 16.0;
/// 4→(1,2,3): one modulator fanned out to three carriers, clusters.
const FAN: f32 = 32.0;
/// 2→1 · 3 · 4: a pair plus two sines.
const PAIR_SINES: f32 = 64.0;
/// 1 · 2 · 3 · 4: fully additive.
const ADDITIVE: f32 = 128.0;

const HARM: f32 = crate::params::RATIOS_HARMONIC as f32;
const INHARM: f32 = crate::params::RATIOS_INHARMONIC as f32;

use Param::*;

pub const SCENES: &[Scene] = &[
    Scene {
        name: "Init",
        values: &[],
        origin: None,
    },
    Scene {
        name: "Bell Cathedral",
        origin: Some("Ring Bells"),
        values: &[
            (Algorithms, PAIRS + FAN + PAIR_SINES + ADDITIVE),
            (Ratios, INHARM),
            (Index, 0.4),
            (Pitch, 0.45),
            (Range, 0.5),
            (Scale, 3.0),
            (Stretch, 0.62),
            (Window, 4.0),
            (Smear, 0.2),
            (Focus, 0.72),
            (EchoOn, 0.0),
            (VerbDecay, 0.8),
            (VerbMix, 0.55),
            (Shimmer, 0.6),
        ],
    },
    Scene {
        name: "Gamelan Drift",
        origin: Some("Gamelan"),
        values: &[
            (Algorithms, PAIRS + FAN + ADDITIVE),
            (Ratios, INHARM),
            (Scale, 3.0),
            (Root, 2.0),
            (Pitch, 0.4),
            (Stretch, 0.55),
            (EchoSpacing, 3.0),
            (EchoFeedback, 0.6),
            (EchoTime, 0.66),
            (Shimmer, 0.3),
            (VerbDecay, 0.7),
        ],
    },
    Scene {
        name: "Organ of Iron",
        origin: Some("Industrial"),
        values: &[
            (Algorithms, STACKS),
            (Ratios, HARM),
            (Index, 0.7),
            (Grit, 0.5),
            (Pitch, 0.22),
            (Range, 0.3),
            (Scale, 1.0),
            (Root, 2.0),
            (Stretch, 0.5),
            (Sub, 0.5),
            (Tilt, 0.3),
            (Lock, 0.5),
            (EchoTime, 0.55),
            (EchoFeedback, 0.7),
            (EchoLevel, 0.4),
            (EchoSpacing, 1.0),
            (VerbMix, 0.35),
            (Shimmer, 0.0),
        ],
    },
    Scene {
        name: "Cross Currents",
        origin: Some("Cross Clang"),
        values: &[
            (Cross, 1.0),
            (Stretch, 0.72),
            (Smear, 0.3),
            (Focus, 0.35),
            (Range, 0.8),
            (Scale, 0.0),
            (EchoMode, 2.0),
            (EchoTone, 0.6),
            (EchoFeedback, 0.75),
            (EchoSpacing, 3.0),
            (Shimmer, 0.4),
            (ShimmerInterval, 5.0),
            (VerbDecay, 0.78),
        ],
    },
    Scene {
        name: "Sine Choir",
        origin: Some("Ring Bells"),
        values: &[
            (Algorithms, PAIR_SINES + ADDITIVE),
            (Ratios, HARM),
            (Index, 0.12),
            (Cross, 0.0),
            (Scale, 6.0),
            (Root, 4.0),
            (Smear, 0.35),
            (Focus, 0.8),
            (Stretch, 0.75),
            (Shimmer, 0.7),
            (ShimmerInterval, 2.0),
            (VerbDecay, 0.85),
            (VerbMix, 0.6),
        ],
    },
    Scene {
        name: "Dub Chamber",
        origin: Some("Body Snare"),
        values: &[
            (Algorithms, PAIRS + FAN + PAIR_SINES),
            (Ratios, HARM),
            (Pitch, 0.4),
            (Range, 0.35),
            (Scale, 4.0),
            (Root, 5.0),
            (Stretch, 0.35),
            (Window, 2.0),
            (StepTime, 0.145),
            (Steps, 16.0),
            (Pulses, 5.0),
            (EchoSync, 1.0),
            (EchoTime, 0.636),
            (EchoMode, 1.0),
            (EchoTone, 0.45),
            (EchoFeedback, 0.72),
            (EchoSpacing, 1.0),
            (EchoLevel, 0.5),
            (VerbMix, 0.4),
            (Shimmer, 0.1),
        ],
    },
    Scene {
        name: "Chaos Pair",
        origin: Some("Chaos Pair"),
        values: &[
            (Grit, 0.9),
            (Index, 1.0),
            (Cross, 0.8),
            (Range, 1.0),
            (Scale, 0.0),
            (Stretch, 0.45),
            (Window, 1.0),
            (Focus, 0.2),
            (Chance, 0.7),
            (Steps, 7.0),
            (Pulses, 4.0),
            (StepTime, 0.35),
            (EchoMode, 3.0),
            (EchoFeedback, 0.8),
            (EchoTone, 0.9),
            (VerbMix, 0.5),
            (Shimmer, 0.2),
        ],
    },
    Scene {
        name: "Deep Carrier",
        origin: Some("Kick & Hat"),
        values: &[
            (Algorithms, PAIRS + PAIR_SINES + ADDITIVE),
            (Ratios, HARM),
            (Index, 0.25),
            (Pitch, 0.2),
            (Range, 0.1),
            (Scale, 1.0),
            (Sub, 0.8),
            (Lock, 0.8),
            (Tilt, 0.3),
            (Stretch, 0.6),
            (Window, 5.0),
            (EchoTime, 0.8),
            (EchoFeedback, 0.4),
            (EchoLevel, 0.25),
            (VerbMix, 0.3),
            (Shimmer, 0.0),
        ],
    },
    Scene {
        name: "Frozen Glass",
        origin: Some("Ring Bells"),
        values: &[
            (Mutate, 0.4),
            (Ratios, INHARM),
            (Scale, 2.0),
            (Stretch, 1.0),
            (Window, 5.0),
            (StepTime, 1.0),
            (Steps, 1.0),
            (Pulses, 1.0),
            (Length, 0.8),
            (EchoOn, 0.0),
            (Shimmer, 0.45),
            (VerbDecay, 1.0),
            (VerbMix, 0.5),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_are_sane() {
        let current = default_patch();
        for s in SCENES {
            let p = s.patch(&current);
            for param in crate::params::ALL_PARAMS {
                assert_eq!(
                    param.sanitize(p[param.index()]),
                    p[param.index()],
                    "{} {param:?}",
                    s.name
                );
            }
            if let Some(o) = s.origin {
                assert_ne!(origin_index(o), 0, "{}: unknown FK-2 kit {o}", s.name);
            }
            assert!(p[Param::Algorithms.index()] >= 1.0);
        }
    }
}
