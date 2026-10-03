//! Parameter definitions shared by the machine, the wasm worklet and the UI.
//!
//! Every parameter is stored as an `f32`:
//!
//! * [`ParamKind::Continuous`]: a knob position in `0.0..=1.0`, turned into
//!   physical units by the [`map`] module (used by both the DSP and the panel
//!   read-outs, so what the panel shows is exactly what the engine does);
//! * [`ParamKind::Choice`]: the index of the selected option;
//! * [`ParamKind::Int`]: an integer value;
//! * [`ParamKind::Toggle`]: `0.0` or `1.0`.
//!
//! A few read-outs depend on other parameters (the echo's TONE knob means
//! something different in every mode, a synced echo time depends on the
//! pattern's step time), so [`Param::label`] and [`Param::display`] take the
//! whole [`Patch`].

use frekussion_dsp::presets::KITS;

/// The discriminant is the stable wire id.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    // --- pattern: when new hits are struck -------------------------------
    /// Duration of one pattern step.
    StepTime = 0,
    /// Euclidean pattern length.
    Steps,
    /// Pulses spread as evenly as possible over the steps.
    Pulses,
    /// Pattern rotation.
    Rotate,
    /// Probability that a pulse actually strikes.
    Chance,

    // --- source: the headless FK-2 --------------------------------------
    /// FK-2 MUTATE applied to both channels on every strike.
    Mutate,
    /// Bit mask of the FK-2 algorithms MUTATE may pick.
    Algorithms,
    /// FK-2 kit the two channels start from (and return to when changed).
    Origin,
    /// Centre pitch of the struck sounds.
    Pitch,
    /// Random pitch spread around the centre.
    Range,
    /// Scale the random pitches snap to.
    Scale,
    /// Key of the scale (and of the harmonic LOCK).
    Root,
    /// FK-2 amp decay: how long each struck sound rings.
    Length,
    /// Ceiling on FK-2's drive, noise and operator feedback.
    Grit,
    /// Which operator frequency ratios are allowed: all, harmonic only or
    /// inharmonic only.
    Ratios,
    /// Scales every modulator's level: FM brightness.
    Index,
    /// Scales the cross-channel XMOD and RING the mutation draws.
    Cross,

    // --- stretch -----------------------------------------------------------
    /// Time-stretch factor.
    Stretch,
    /// Analysis window (FFT size).
    Window,
    /// Stereo phase decorrelation.
    Width,
    /// Stop time: hold every layer where it is.
    Freeze,

    // --- spectral ------------------------------------------------------------
    /// Spectral transposition, semitones.
    Shift,
    /// Blend of the spectrum an octave down.
    Sub,
    /// Spectral blur.
    Smear,
    /// Noise (flattened spectrum) ↔ tonal (sharpened peaks), bipolar.
    Focus,
    /// Filter the spectrum towards the harmonics of ROOT.
    Lock,
    /// Spectral tilt, bipolar.
    Tilt,

    // --- echo ------------------------------------------------------------------
    EchoOn,
    /// Left delay time, or a division of the step when synced.
    EchoTime,
    /// Sync the echo time to the pattern's step.
    EchoSync,
    /// Right/left delay time ratio.
    EchoSpacing,
    /// What happens to every repeat.
    EchoMode,
    /// The mode's colour control.
    EchoTone,
    EchoFeedback,
    EchoLevel,

    // --- reverb ------------------------------------------------------------------
    VerbOn,
    VerbMix,
    VerbDecay,
    VerbTone,
    VerbPre,
    /// Pitch shifting inside the plate's tank.
    Shimmer,
    /// The shimmer's interval.
    ShimmerInterval,

    // --- output ----------------------------------------------------------------------
    Master,
}

pub const PARAM_COUNT: usize = 43;

pub const ALL_PARAMS: [Param; PARAM_COUNT] = {
    use Param::*;
    [
        StepTime,
        Steps,
        Pulses,
        Rotate,
        Chance,
        Mutate,
        Algorithms,
        Origin,
        Pitch,
        Range,
        Scale,
        Root,
        Length,
        Grit,
        Ratios,
        Index,
        Cross,
        Stretch,
        Window,
        Width,
        Freeze,
        Shift,
        Sub,
        Smear,
        Focus,
        Lock,
        Tilt,
        EchoOn,
        EchoTime,
        EchoSync,
        EchoSpacing,
        EchoMode,
        EchoTone,
        EchoFeedback,
        EchoLevel,
        VerbOn,
        VerbMix,
        VerbDecay,
        VerbTone,
        VerbPre,
        Shimmer,
        ShimmerInterval,
        Master,
    ]
};

/// One value per [`Param`], indexed by [`Param::index`].
pub type Patch = [f32; PARAM_COUNT];

/// All eight FK-2 algorithms.
pub const ALL_ALGORITHMS: u32 = 0xff;

pub const RATIO_SET_NAMES: &[&str] = &["ALL", "HARM", "INHARM"];
pub const RATIOS_ALL: u32 = 0;
pub const RATIOS_HARMONIC: u32 = 1;
pub const RATIOS_INHARMONIC: u32 = 2;

pub const WINDOW_NAMES: &[&str] = &["2K", "4K", "8K", "16K", "32K", "64K"];
/// log2 of the smallest window.
pub const WINDOW_MIN_LOG2: u32 = 11;
pub const WINDOW_MAX_LOG2: u32 = WINDOW_MIN_LOG2 + WINDOW_NAMES.len() as u32 - 1;

pub const SCALE_NAMES: &[&str] = &[
    "FREE", "OCT", "5TH", "PENTA", "MINOR", "DORIAN", "LYDIAN", "WHOLE",
];
/// Semitones above the root of each scale (FREE has none: no snapping).
pub const SCALES: [&[u8]; 8] = [
    &[],
    &[0],
    &[0, 7],
    &[0, 3, 5, 7, 10],
    &[0, 2, 3, 5, 7, 8, 10],
    &[0, 2, 3, 5, 7, 9, 10],
    &[0, 2, 4, 6, 7, 9, 11],
    &[0, 2, 4, 6, 8, 10],
];
pub const SCALE_FREE: u32 = 0;

pub const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub const SPACING_NAMES: &[&str] = &["EVEN", "DOTTED", "TRIPLET", "GOLDEN", "SILVER"];
/// Right/left delay ratio per spacing. The irrational ones never line up
/// with the left side again, so the ping-pong smears into an irregular
/// cloud of repeats.
pub const SPACING_RATIOS: [f32; 5] = [
    1.0,
    0.75,
    2.0 / 3.0,
    0.618_034,
    core::f32::consts::FRAC_1_SQRT_2,
];

pub const ECHO_MODE_NAMES: &[&str] = &["TAPE", "BAND", "SPIRAL", "DIFFUSE"];
pub const ECHO_TAPE: u32 = 0;
pub const ECHO_BAND: u32 = 1;
pub const ECHO_SPIRAL: u32 = 2;
pub const ECHO_DIFFUSE: u32 = 3;

/// Echo time as a fraction of the pattern step when synced.
pub const SYNC_NAMES: &[&str] = &[
    "1/16", "1/12", "1/8", "1/6", "3/16", "1/4", "1/3", "3/8", "1/2", "2/3", "3/4", "1",
];
pub const SYNC_DIVISIONS: [f32; 12] = [
    1.0 / 16.0,
    1.0 / 12.0,
    1.0 / 8.0,
    1.0 / 6.0,
    3.0 / 16.0,
    1.0 / 4.0,
    1.0 / 3.0,
    3.0 / 8.0,
    1.0 / 2.0,
    2.0 / 3.0,
    3.0 / 4.0,
    1.0,
];

pub const SHIMMER_NAMES: &[&str] = &["+OCT", "+5TH", "+OCT+5", "+2OCT", "-OCT", "±OCT"];
/// (left tank half, right tank half) shift in semitones per interval.
pub const SHIMMER_SEMITONES: [(f32, f32); 6] = [
    (12.0, 12.0),
    (7.0, 7.0),
    (19.0, 19.0),
    (24.0, 24.0),
    (-12.0, -12.0),
    (12.0, -12.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Continuous,
    Choice(&'static [&'static str]),
    Int { min: i32, max: i32 },
    Toggle,
}

impl ParamKind {
    /// Stored value → knob position in `0..=1`.
    pub fn to_norm(self, v: f32) -> f32 {
        match self {
            ParamKind::Continuous => v,
            ParamKind::Choice(names) => v / (names.len() - 1).max(1) as f32,
            ParamKind::Int { min, max } => (v - min as f32) / (max - min).max(1) as f32,
            ParamKind::Toggle => v,
        }
    }

    /// Knob position → stored value (unsanitized).
    pub fn from_norm(self, x: f32) -> f32 {
        match self {
            ParamKind::Continuous => x,
            ParamKind::Choice(names) => x * (names.len() - 1) as f32,
            ParamKind::Int { min, max } => min as f32 + x * (max - min) as f32,
            ParamKind::Toggle => x,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    /// Descriptive name.
    pub name: &'static str,
    /// Panel label.
    pub label: &'static str,
    pub default: f32,
    pub kind: ParamKind,
}

/// Index of a factory FK-2 kit by name (0 if it doesn't exist).
pub fn origin_index(name: &str) -> usize {
    KITS.iter().position(|k| k.name == name).unwrap_or(0)
}

impl Param {
    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_id(id: u32) -> Option<Param> {
        ALL_PARAMS.get(id as usize).copied()
    }

    pub fn info(self) -> ParamInfo {
        use Param::*;
        use ParamKind::*;
        let (name, label, default, kind) = match self {
            StepTime => ("Step time", "STEP", 0.52, Continuous),
            Steps => ("Pattern steps", "STEPS", 8.0, Int { min: 1, max: 16 }),
            Pulses => ("Pattern pulses", "PULSES", 3.0, Int { min: 1, max: 16 }),
            Rotate => ("Pattern rotation", "ROTATE", 0.0, Int { min: 0, max: 15 }),
            Chance => ("Strike chance", "CHANCE", 1.0, Continuous),
            Mutate => ("Mutate", "MUTATE", 1.0, Continuous),
            Algorithms => (
                "Algorithms",
                "ALGORITHMS",
                ALL_ALGORITHMS as f32,
                Int {
                    min: 1,
                    max: ALL_ALGORITHMS as i32,
                },
            ),
            Origin => (
                "Origin kit",
                "ORIGIN",
                origin_index("Ring Bells") as f32,
                Int {
                    min: 0,
                    max: KITS.len() as i32 - 1,
                },
            ),
            Pitch => ("Pitch", "PITCH", 0.33, Continuous),
            Range => ("Pitch range", "RANGE", 0.55, Continuous),
            Scale => ("Scale", "SCALE", 3.0, Choice(SCALE_NAMES)),
            Root => ("Root", "ROOT", 9.0, Choice(NOTE_NAMES)),
            Length => ("Hit length", "LENGTH", 0.5, Continuous),
            Grit => ("Grit", "GRIT", 0.25, Continuous),
            Ratios => (
                "Ratio set",
                "RATIOS",
                RATIOS_ALL as f32,
                Choice(RATIO_SET_NAMES),
            ),
            Index => ("FM index", "INDEX", 0.5, Continuous),
            Cross => ("Cross modulation", "CROSS", 0.5, Continuous),
            Stretch => ("Stretch", "STRETCH", 0.566_3, Continuous),
            Window => ("Window", "WINDOW", 3.0, Choice(WINDOW_NAMES)),
            Width => ("Width", "WIDTH", 0.85, Continuous),
            Freeze => ("Freeze", "FREEZE", 0.0, Toggle),
            Shift => ("Transpose", "SHIFT", 0.0, Int { min: -24, max: 24 }),
            Sub => ("Sub octave", "SUB", 0.0, Continuous),
            Smear => ("Smear", "SMEAR", 0.15, Continuous),
            Focus => ("Focus", "FOCUS", 0.5, Continuous),
            Lock => ("Harmonic lock", "LOCK", 0.0, Continuous),
            Tilt => ("Tilt", "TILT", 0.45, Continuous),
            EchoOn => ("Echo", "ECHO", 1.0, Toggle),
            EchoTime => ("Echo time", "TIME", 0.62, Continuous),
            EchoSync => ("Echo sync", "SYNC", 0.0, Toggle),
            EchoSpacing => ("Echo spacing", "SPACING", 3.0, Choice(SPACING_NAMES)),
            EchoMode => (
                "Echo mode",
                "MODE",
                ECHO_TAPE as f32,
                Choice(ECHO_MODE_NAMES),
            ),
            EchoTone => ("Echo tone", "TONE", 0.55, Continuous),
            EchoFeedback => ("Echo feedback", "FEEDBK", 0.5, Continuous),
            EchoLevel => ("Echo level", "LEVEL", 0.35, Continuous),
            VerbOn => ("Reverb", "REVERB", 1.0, Toggle),
            VerbMix => ("Reverb mix", "MIX", 0.45, Continuous),
            VerbDecay => ("Reverb decay", "DECAY", 0.664, Continuous),
            VerbTone => ("Reverb tone", "TONE", 0.5, Continuous),
            VerbPre => ("Reverb pre-delay", "PRE", 0.15, Continuous),
            Shimmer => ("Shimmer", "SHIMMER", 0.35, Continuous),
            ShimmerInterval => ("Shimmer interval", "INTERVAL", 0.0, Choice(SHIMMER_NAMES)),
            Master => ("Master", "MASTER", 0.8, Continuous),
        };
        ParamInfo {
            name,
            label,
            default,
            kind,
        }
    }

    pub fn kind(self) -> ParamKind {
        self.info().kind
    }

    pub fn default_value(self) -> f32 {
        self.info().default
    }

    /// Clamp/quantize a raw value to what this parameter accepts.
    pub fn sanitize(self, v: f32) -> f32 {
        let v = if v.is_finite() {
            v
        } else {
            self.default_value()
        };
        match self.kind() {
            ParamKind::Continuous => v.clamp(0.0, 1.0),
            ParamKind::Choice(names) => v.round().clamp(0.0, (names.len() - 1) as f32),
            ParamKind::Int { min, max } => v.round().clamp(min as f32, max as f32),
            ParamKind::Toggle => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// Bipolar controls are centred (the panel lights them from the middle).
    pub fn is_bipolar(self) -> bool {
        matches!(self, Param::Focus | Param::Tilt | Param::Shift)
    }

    /// Does changing this parameter change the sound of the struck hits
    /// (so already rendered hits are out of date)?
    pub fn shapes_source(self) -> bool {
        use Param::*;
        matches!(
            self,
            Mutate
                | Algorithms
                | Origin
                | Pitch
                | Range
                | Scale
                | Root
                | Length
                | Grit
                | Ratios
                | Index
                | Cross
        )
    }

    /// Panel label; the echo's colour knob is named after the mode.
    pub fn label(self, patch: &Patch) -> &'static str {
        if self == Param::EchoTone {
            return ["TONE", "CENTER", "SHIFT", "BLUR"][echo_mode(patch) as usize];
        }
        self.info().label
    }

    /// Human readable value for the panel read-out.
    pub fn display(self, v: f32, patch: &Patch) -> String {
        use Param::*;
        let v = self.sanitize(v);
        match self {
            StepTime => fmt_secs(map::step_seconds(v)),
            Steps | Pulses | Rotate => format!("{}", v as i32),
            Chance | Mutate | Width | Sub | Smear | Lock | Grit | Shimmer | VerbMix | EchoLevel => {
                pct_or_off(v)
            }
            Algorithms => format!("{} of 8", (v as u32).count_ones()),
            Origin => KITS
                .get(v as usize)
                .map(|k| k.name.to_uppercase())
                .unwrap_or_default(),
            Index => format!("x{:.2}", map::index_scale(v)),
            Cross => {
                if v <= 0.0 {
                    "OFF".into()
                } else {
                    format!("x{:.2}", map::cross_scale(v))
                }
            }
            Pitch => fmt_hz(map::pitch_hz(v)),
            Range => {
                let o = map::range_octaves(v);
                if o < 0.01 {
                    "FIXED".into()
                } else {
                    format!("±{o:.1} oct")
                }
            }
            Scale | Root | Window | EchoSpacing | EchoMode | ShimmerInterval | Ratios => {
                match self.kind() {
                    ParamKind::Choice(names) => names[v as usize].to_string(),
                    _ => unreachable!(),
                }
            }
            Length => fmt_secs(frekussion_dsp::params::map::decay_seconds(
                map::length_decay_knob(v),
            )),
            Stretch => {
                let s = map::stretch_factor(v);
                if s < 10.0 {
                    format!("{s:.2}x")
                } else if s < 100.0 {
                    format!("{s:.1}x")
                } else {
                    format!("{s:.0}x")
                }
            }
            Freeze | EchoOn | EchoSync | VerbOn => if v > 0.5 { "ON" } else { "OFF" }.into(),
            Shift => {
                if v == 0.0 {
                    "0 st".into()
                } else {
                    format!("{:+} st", v as i32)
                }
            }
            Focus => {
                let f = map::focus_exponent(v);
                if f.abs() < 0.02 {
                    "NEUTRAL".into()
                } else if f < 0.0 {
                    format!("NOISE {:.0}%", -f * 100.0)
                } else {
                    format!("TONAL {:.0}%", f / 2.0 * 100.0)
                }
            }
            Tilt => format!("{:+.1} dB/oct", map::tilt_db_per_octave(v)),
            EchoTime => {
                if patch[EchoSync.index()] > 0.5 {
                    let i = map::sync_index(v);
                    let secs = map::step_seconds(patch[StepTime.index()]) * SYNC_DIVISIONS[i];
                    format!(
                        "{} · {}",
                        SYNC_NAMES[i],
                        fmt_secs(secs.min(map::ECHO_MAX_S))
                    )
                } else {
                    fmt_secs(map::echo_seconds(v))
                }
            }
            EchoTone => match echo_mode(patch) {
                ECHO_TAPE => fmt_hz(map::echo_tape_cutoff_hz(v)),
                ECHO_BAND => fmt_hz(map::echo_band_hz(v)),
                ECHO_SPIRAL => format!("{:+.1} Hz", map::echo_spiral_hz(v)),
                _ => pct_or_off(v),
            },
            EchoFeedback => format!("{:.0}%", map::echo_feedback(v) * 100.0),
            VerbDecay => fmt_secs(map::verb_seconds(v)),
            VerbTone => fmt_hz(map::verb_tone_hz(v)),
            VerbPre => fmt_secs(map::verb_predelay_seconds(v)),
            Master => fmt_db(map::master_gain(v)),
        }
    }
}

fn echo_mode(patch: &Patch) -> u32 {
    Param::EchoMode.sanitize(patch[Param::EchoMode.index()]) as u32
}

pub fn default_patch() -> Patch {
    let mut p = [0.0; PARAM_COUNT];
    for param in ALL_PARAMS {
        p[param.index()] = param.default_value();
    }
    p
}

fn pct_or_off(v: f32) -> String {
    if v <= 0.0 {
        "OFF".into()
    } else {
        format!("{:.0}%", v * 100.0)
    }
}

pub fn fmt_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else if hz >= 10.0 {
        format!("{:.0} Hz", hz)
    } else {
        format!("{:.2} Hz", hz)
    }
}

pub fn fmt_db(gain: f32) -> String {
    if gain <= 1e-4 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * gain.log10())
    }
}

pub fn fmt_secs(s: f32) -> String {
    if s >= 100.0 {
        format!("{:.0} s", s)
    } else if s >= 10.0 {
        format!("{:.1} s", s)
    } else if s >= 1.0 {
        format!("{:.2} s", s)
    } else if s >= 0.01 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{:.1} ms", s * 1000.0)
    }
}

/// Knob position → physical unit mappings.
pub mod map {
    use super::{SCALE_FREE, SCALES};

    #[inline]
    fn bipolar(v: f32) -> f32 {
        (v - 0.5) * 2.0
    }

    /// Longest echo delay (either side), seconds.
    pub const ECHO_MAX_S: f32 = 2.5;
    /// Largest stretch factor.
    pub const STRETCH_MAX: f32 = 1000.0;

    /// Pattern step: 0.25 s … 30 s.
    pub fn step_seconds(v: f32) -> f32 {
        0.25 * 120f32.powf(v)
    }

    /// Same curve as FK-2's TUNE fader: 20 Hz … 2.56 kHz.
    pub fn pitch_hz(v: f32) -> f32 {
        frekussion_dsp::params::map::tune_hz(v, 0.5)
    }

    /// Inverse of [`pitch_hz`]: the FK-2 TUNE fader position for `hz`.
    pub fn pitch_knob(hz: f32) -> f32 {
        use frekussion_dsp::params::map::{TUNE_MIN_HZ, TUNE_OCTAVES};
        ((hz / TUNE_MIN_HZ).log2() / TUNE_OCTAVES).clamp(0.0, 1.0)
    }

    /// Random pitch spread, ± octaves.
    pub fn range_octaves(v: f32) -> f32 {
        4.0 * v * v
    }

    /// Frequency of `root` (0 = C) in octave `octave` (A4 = 440 Hz).
    pub fn note_hz(root: u32, octave: i32) -> f32 {
        let midi = (octave + 1) * 12 + root as i32;
        440.0 * ((midi - 69) as f32 / 12.0).exp2()
    }

    /// Snap `hz` to the nearest note of `scale` in the key of `root`.
    pub fn quantize_hz(hz: f32, root: u32, scale: u32) -> f32 {
        let degrees = SCALES[(scale as usize).min(SCALES.len() - 1)];
        if scale == SCALE_FREE || degrees.is_empty() || hz <= 0.0 {
            return hz;
        }
        // Semitones above C-1 of the root.
        let base = note_hz(root, -1);
        let semis = 12.0 * (hz / base).log2();
        let octave = (semis / 12.0).floor();
        let mut best = f32::MAX;
        let mut best_semis = semis;
        // Check this octave's degrees and the next octave's root.
        for o in [octave - 1.0, octave, octave + 1.0] {
            for &d in degrees {
                let cand = o * 12.0 + d as f32;
                let dist = (cand - semis).abs();
                if dist < best {
                    best = dist;
                    best_semis = cand;
                }
            }
        }
        base * (best_semis / 12.0).exp2()
    }

    /// LENGTH → FK-2 amp DECAY knob position (≈ 130 ms … 10 s of ring).
    pub fn length_decay_knob(v: f32) -> f32 {
        0.35 + 0.6 * v
    }

    /// GRIT → largest FK-2 DRIVE and N>FM a mutation may produce.
    pub fn grit_ceiling(v: f32) -> f32 {
        v
    }

    /// GRIT → largest operator-4 FEEDBACK: clean saw-like tones at 0, all the
    /// way into FK-2's deliberate noise at 1.
    pub fn feedback_ceiling(v: f32) -> f32 {
        0.3 + 0.7 * v
    }

    /// INDEX → modulator LEVEL knob multiplier (0 … 2). The modulation index
    /// goes with the square of the knob, so this is 0 … 4× the index.
    pub fn index_knob_scale(v: f32) -> f32 {
        2.0 * v
    }

    /// INDEX as shown: the factor on the modulation index.
    pub fn index_scale(v: f32) -> f32 {
        index_knob_scale(v).powi(2)
    }

    /// CROSS → XMOD/RING knob multiplier (0 … 2).
    pub fn cross_knob_scale(v: f32) -> f32 {
        2.0 * v
    }

    /// CROSS as shown: the factor on the cross-modulation index.
    pub fn cross_scale(v: f32) -> f32 {
        cross_knob_scale(v).powi(2)
    }

    /// 1x … 1000x.
    pub fn stretch_factor(v: f32) -> f32 {
        STRETCH_MAX.powf(v)
    }

    /// Knob position for a stretch factor.
    pub fn stretch_knob(factor: f32) -> f32 {
        (factor.max(1.0).ln() / STRETCH_MAX.ln()).clamp(0.0, 1.0)
    }

    /// log2 of the FFT size for a WINDOW choice.
    pub fn window_log2(choice: f32) -> u32 {
        super::WINDOW_MIN_LOG2 + (choice.round().max(0.0) as u32).min(5)
    }

    /// Spectral blur bandwidth, octaves.
    pub fn smear_octaves(v: f32) -> f32 {
        0.5 * v * v
    }

    /// FOCUS exponent: −1 (flatten to noise) … +2 (sharpen to tones).
    pub fn focus_exponent(v: f32) -> f32 {
        let x = bipolar(v);
        if x < 0.0 { x } else { 2.0 * x }
    }

    /// ±6 dB per octave around [`TILT_PIVOT_HZ`].
    pub fn tilt_db_per_octave(v: f32) -> f32 {
        6.0 * bipolar(v)
    }
    pub const TILT_PIVOT_HZ: f32 = 600.0;

    /// Echo time (free): 20 ms … 2.5 s.
    pub fn echo_seconds(v: f32) -> f32 {
        0.02 * 125f32.powf(v)
    }

    /// Echo time (synced): index into [`super::SYNC_DIVISIONS`].
    pub fn sync_index(v: f32) -> usize {
        ((v.clamp(0.0, 1.0) * (super::SYNC_DIVISIONS.len() - 1) as f32).round() as usize)
            .min(super::SYNC_DIVISIONS.len() - 1)
    }

    /// Up to 110%: past 100% TAPE runs away into (soft-clipped) self-oscillation.
    pub fn echo_feedback(v: f32) -> f32 {
        1.1 * v
    }

    pub fn echo_tape_cutoff_hz(v: f32) -> f32 {
        500.0 * 30f32.powf(v)
    }

    pub fn echo_band_hz(v: f32) -> f32 {
        150.0 * 40f32.powf(v)
    }

    /// Frequency shift per repeat, bipolar: ±30 Hz.
    pub fn echo_spiral_hz(v: f32) -> f32 {
        let x = bipolar(v);
        30.0 * x * x.abs()
    }

    /// Allpass gain of the DIFFUSE mode's smearing network.
    pub fn echo_diffusion(v: f32) -> f32 {
        0.75 * v
    }

    /// Reverb RT60: 0.5 s … 60 s.
    pub fn verb_seconds(v: f32) -> f32 {
        0.5 * 120f32.powf(v)
    }

    pub fn verb_tone_hz(v: f32) -> f32 {
        1500.0 * 12f32.powf(v)
    }

    pub fn verb_predelay_seconds(v: f32) -> f32 {
        0.4 * v
    }

    /// How much of each tank pass goes through the pitch shifter.
    pub fn shimmer_mix(v: f32) -> f32 {
        0.8 * v
    }

    pub fn master_gain(v: f32) -> f32 {
        v * v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip() {
        for (i, p) in ALL_PARAMS.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(Param::from_id(p.id()), Some(*p));
        }
        assert_eq!(Param::from_id(PARAM_COUNT as u32), None);
    }

    #[test]
    fn defaults_are_sanitized() {
        for p in ALL_PARAMS {
            assert_eq!(p.sanitize(p.default_value()), p.default_value(), "{p:?}");
        }
    }

    #[test]
    fn norm_roundtrip() {
        for p in ALL_PARAMS {
            let k = p.kind();
            let v = p.default_value();
            assert_eq!(p.sanitize(k.from_norm(k.to_norm(v))), v, "{p:?}");
        }
    }

    #[test]
    fn mappings() {
        assert!((map::stretch_factor(0.0) - 1.0).abs() < 1e-6);
        assert!((map::stretch_factor(1.0) - 1000.0).abs() < 0.1);
        let fifty = map::stretch_factor(Param::Stretch.default_value());
        assert!((fifty - 50.0).abs() < 0.1, "{fifty}");
        assert!((map::stretch_knob(50.0) - Param::Stretch.default_value()).abs() < 1e-3);
        assert!((map::step_seconds(1.0) - 30.0).abs() < 0.01);
        assert!((map::echo_seconds(1.0) - map::ECHO_MAX_S).abs() < 1e-3);
        assert!((map::verb_seconds(1.0) - 60.0).abs() < 0.01);
        assert_eq!(map::window_log2(0.0), 11);
        assert_eq!(map::window_log2(5.0), WINDOW_MAX_LOG2);
        assert!((map::pitch_knob(map::pitch_hz(0.37)) - 0.37).abs() < 1e-5);
        assert_eq!(map::echo_spiral_hz(0.5), 0.0);
    }

    #[test]
    fn quantization_snaps_to_the_scale() {
        let a = map::note_hz(9, 3); // A3
        assert!((a - 220.0).abs() < 0.01);
        // Slightly sharp A snaps to A.
        assert!((map::quantize_hz(226.0, 9, 3) - 220.0).abs() < 0.01);
        // In A minor pentatonic B (246.9) is not a degree: nearest are A and C.
        let q = map::quantize_hz(246.9, 9, 3);
        assert!((q - 220.0).abs() < 0.01 || (q - 261.63).abs() < 0.05, "{q}");
        // Octaves only.
        assert!((map::quantize_hz(300.0, 9, 1) - 220.0).abs() < 0.01);
        assert!((map::quantize_hz(330.0, 9, 1) - 440.0).abs() < 0.01);
        // FREE leaves it alone.
        assert_eq!(map::quantize_hz(300.0, 9, SCALE_FREE), 300.0);
    }

    #[test]
    fn every_display_is_nonempty() {
        let patch = default_patch();
        for p in ALL_PARAMS {
            for x in [0.0, 0.3, 0.5, 1.0] {
                let v = p.sanitize(p.kind().from_norm(x));
                assert!(!p.display(v, &patch).is_empty(), "{p:?}");
            }
            assert!(!p.label(&patch).is_empty());
        }
        let mut synced = patch;
        synced[Param::EchoSync.index()] = 1.0;
        assert!(Param::EchoTime.display(1.0, &synced).starts_with("1 ·"));
    }
}
