//! Parameter definitions shared by the engine, the wasm worklet and the UI.
//!
//! Every parameter is stored as an `f32`. Continuous parameters are normalized
//! to `0.0..=1.0` (think "knob position"); choice parameters hold the index of
//! the selected option. The [`map`] module converts knob positions into
//! physical units and is used by both the DSP and the UI read-outs, so what the
//! panel displays is exactly what the engine does.

/// Number of FM operators per channel.
pub const OPS: usize = 4;

/// Per-voice parameters. The discriminant is the stable wire id.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    /// FM algorithm (operator routing), see [`ALGORITHMS`].
    Algorithm = 0,
    /// Coarse tune fader: the pitch every operator ratio is relative to.
    Tune,
    /// Fine tune.
    Fine,
    /// Operator 4 self-feedback.
    Feedback,
    /// Amp envelope decay.
    Decay,
    /// Pitch sweep depth (bipolar: below centre sweeps up into the note,
    /// above centre drops down onto it).
    SweepAmount,
    /// Pitch sweep time.
    SweepTime,
    /// How much velocity scales the modulation index.
    VelFm,
    /// Saturation before the filter.
    Drive,
    /// Bipolar filter: lowpass below centre, highpass above, open at centre.
    Filter,
    /// Filter resonance.
    Resonance,
    /// Channel output level.
    Level,
    /// Trigger sensitivity.
    Sense,
    /// Stereo position.
    Pan,

    Op1Ratio,
    Op1Fine,
    Op1Level,
    Op1Decay,
    Op2Ratio,
    Op2Fine,
    Op2Level,
    Op2Decay,
    Op3Ratio,
    Op3Fine,
    Op3Level,
    Op3Decay,
    Op4Ratio,
    Op4Fine,
    Op4Level,
    Op4Decay,

    /// Noise oscillator level.
    NoiseLevel,
    /// Noise envelope decay.
    NoiseDecay,
    /// Noise filter cutoff.
    NoiseTone,
    /// Noise filter resonance.
    NoiseRes,
    /// Noise filter type: LP / BP / HP.
    NoiseType,
    /// How much the noise phase-modulates the carriers.
    NoiseFm,

    /// LFO rate.
    LfoRate,
    /// LFO depth.
    LfoDepth,
    /// LFO waveform.
    LfoWave,
    /// LFO destination: pitch, modulation index or amplitude.
    LfoDest,

    /// How much the other channel phase-modulates this one.
    XmodAmount,
    /// Which operators the cross modulation reaches.
    XmodDest,
    /// Ring modulation with the other channel.
    Ring,
    /// Fire this channel whenever the other one is triggered.
    Link,

    /// Reverb send level (post channel output level).
    ReverbMix,
    /// Reverb decay time (≈ RT60).
    ReverbDecay,
    /// Reverb damping / brightness.
    ReverbTone,
    /// Reverb pre-delay.
    ReverbPredelay,
}

pub const PARAM_COUNT: usize = 48;

pub const ALL_PARAMS: [Param; PARAM_COUNT] = {
    use Param::*;
    [
        Algorithm,
        Tune,
        Fine,
        Feedback,
        Decay,
        SweepAmount,
        SweepTime,
        VelFm,
        Drive,
        Filter,
        Resonance,
        Level,
        Sense,
        Pan,
        Op1Ratio,
        Op1Fine,
        Op1Level,
        Op1Decay,
        Op2Ratio,
        Op2Fine,
        Op2Level,
        Op2Decay,
        Op3Ratio,
        Op3Fine,
        Op3Level,
        Op3Decay,
        Op4Ratio,
        Op4Fine,
        Op4Level,
        Op4Decay,
        NoiseLevel,
        NoiseDecay,
        NoiseTone,
        NoiseRes,
        NoiseType,
        NoiseFm,
        LfoRate,
        LfoDepth,
        LfoWave,
        LfoDest,
        XmodAmount,
        XmodDest,
        Ring,
        Link,
        ReverbMix,
        ReverbDecay,
        ReverbTone,
        ReverbPredelay,
    ]
};

/// The four per-operator controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpParam {
    Ratio,
    Fine,
    Level,
    Decay,
}

impl OpParam {
    pub const ALL: [OpParam; 4] = [
        OpParam::Ratio,
        OpParam::Fine,
        OpParam::Level,
        OpParam::Decay,
    ];

    /// The voice parameter for this control on operator `op` (`0..OPS`).
    pub fn of(self, op: usize) -> Param {
        let base = Param::Op1Ratio as usize + 4 * op.min(OPS - 1);
        ALL_PARAMS[base + self as usize]
    }
}

/// Which operator (`0..OPS`) and control a parameter belongs to, if any.
pub fn op_param(p: Param) -> Option<(usize, OpParam)> {
    let i = p as usize;
    let base = Param::Op1Ratio as usize;
    (base..base + 4 * OPS)
        .contains(&i)
        .then(|| ((i - base) / 4, OpParam::ALL[(i - base) % 4]))
}

/// An FM algorithm: who modulates whom. Operators are numbered 1–4 on the
/// panel and `0..4` here; modulation always flows from a higher-numbered
/// operator to a lower one, so rendering operator 4 first and operator 1 last
/// resolves every connection within the same sample. Operator 4 carries the
/// feedback loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Algorithm {
    /// `mods[i]` has bit `j` set when operator `j` modulates operator `i`.
    pub mods: [u8; OPS],
    /// Bit `i` set when operator `i` is heard (a carrier).
    pub carriers: u8,
    pub name: &'static str,
}

impl Algorithm {
    pub fn is_carrier(&self, op: usize) -> bool {
        self.carriers & (1 << op) != 0
    }

    pub fn modulates(&self, from: usize, to: usize) -> bool {
        self.mods[to] & (1 << from) != 0
    }

    pub fn carrier_count(&self) -> u32 {
        self.carriers.count_ones()
    }
}

const fn algo(mods: [u8; OPS], carriers: u8, name: &'static str) -> Algorithm {
    Algorithm {
        mods,
        carriers,
        name,
    }
}

const O2: u8 = 1 << 1;
const O3: u8 = 1 << 2;
const O4: u8 = 1 << 3;

/// The eight algorithms, from deep stacks (noisy, clangorous) to fully
/// additive (bells, chords, 808-style tone pairs).
pub const ALGORITHMS: [Algorithm; 8] = [
    algo([O2, O3, O4, 0], 0b0001, "4→3→2→1"),
    algo([O2, O3 | O4, 0, 0], 0b0001, "(3+4)→2→1"),
    algo([O2 | O3, 0, O4, 0], 0b0001, "(2 + 4→3)→1"),
    algo([O2 | O3 | O4, 0, 0, 0], 0b0001, "(2+3+4)→1"),
    algo([O2, 0, O4, 0], 0b0101, "2→1 · 4→3"),
    algo([O4, O4, O4, 0], 0b0111, "4→(1,2,3)"),
    algo([O2, 0, 0, 0], 0b1101, "2→1 · 3 · 4"),
    algo([0, 0, 0, 0], 0b1111, "1 · 2 · 3 · 4"),
];

pub const ALGORITHM_NAMES: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8"];
pub const NOISE_TYPE_NAMES: &[&str] = &["LP", "BP", "HP"];
pub const LFO_WAVE_NAMES: &[&str] = &["TRI", "SQR", "SAW", "S&H"];
pub const LFO_DEST_NAMES: &[&str] = &["PITCH", "FM", "AMP"];
pub const XMOD_DEST_NAMES: &[&str] = &["CAR", "MOD", "ALL"];
pub const ON_OFF_NAMES: &[&str] = &["OFF", "ON"];

pub const NOISE_LP: u32 = 0;
pub const NOISE_BP: u32 = 1;
pub const NOISE_HP: u32 = 2;
pub const LFO_TRI: u32 = 0;
pub const LFO_SQR: u32 = 1;
pub const LFO_SAW: u32 = 2;
pub const LFO_SH: u32 = 3;
pub const LFO_PITCH: u32 = 0;
pub const LFO_FM: u32 = 1;
pub const LFO_AMP: u32 = 2;
pub const XMOD_CARRIERS: u32 = 0;
pub const XMOD_MODULATORS: u32 = 1;
pub const XMOD_ALL: u32 = 2;

/// Operator frequency ratios the RATIO knob steps through. Besides the
/// harmonic ratios and sub-octaves there are the inharmonic partials of real
/// percussion: an ideal circular membrane (1.59, 2.14, 2.30, 2.65, 2.92), a
/// free-free bar or tuning fork (2.76, 5.40, 8.93, 13.34) and a few classic
/// "clang" ratios (√2, √3, π, 7.07).
pub const RATIOS: [f32; 36] = [
    0.25,
    0.5,
    0.75,
    1.0,
    1.41,
    1.5,
    1.59,
    1.73,
    2.0,
    2.14,
    2.3,
    2.5,
    2.65,
    2.76,
    2.92,
    3.0,
    core::f32::consts::PI,
    3.5,
    4.0,
    4.5,
    5.0,
    5.4,
    6.0,
    7.0,
    7.07,
    8.0,
    8.93,
    9.0,
    10.0,
    11.0,
    12.0,
    13.0,
    13.34,
    14.0,
    15.0,
    16.0,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Continuous,
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    pub param: Param,
    /// Descriptive name.
    pub name: &'static str,
    /// Panel label.
    pub label: &'static str,
    pub default: f32,
    pub kind: ParamKind,
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
        use ParamKind::*;
        let (name, label, default, kind) = if let Some((op, which)) = op_param(self) {
            // Defaults: op 1 a full-level carrier, op 2 a moderate modulator
            // one octave up, ops 3 and 4 silent.
            let (name, label, default) = match which {
                OpParam::Ratio => (
                    OP_RATIO_NAMES[op],
                    "RATIO",
                    ratio_knob(if op == 1 { 2.0 } else { 1.0 }),
                ),
                OpParam::Fine => (OP_FINE_NAMES[op], "FINE", 0.5),
                OpParam::Level => (OP_LEVEL_NAMES[op], "LEVEL", [0.85, 0.45, 0.0, 0.0][op]),
                OpParam::Decay => (OP_DECAY_NAMES[op], "DECAY", [0.55, 0.3, 0.3, 0.3][op]),
            };
            (name, label, default, Continuous)
        } else {
            match self {
                Param::Algorithm => ("Algorithm", "ALGO", 0.0, Choice(ALGORITHM_NAMES)),
                Param::Tune => ("Tune", "TUNE", 0.42, Continuous),
                Param::Fine => ("Fine tune", "FINE", 0.5, Continuous),
                Param::Feedback => ("Feedback", "FDBK", 0.0, Continuous),
                Param::Decay => ("Amp decay", "DECAY", 0.45, Continuous),
                Param::SweepAmount => ("Sweep amount", "SWEEP", 0.5, Continuous),
                Param::SweepTime => ("Sweep time", "TIME", 0.35, Continuous),
                Param::VelFm => ("Velocity to FM", "VEL>FM", 0.5, Continuous),
                Param::Drive => ("Drive", "DRIVE", 0.0, Continuous),
                Param::Filter => ("Filter", "FILTER", 0.5, Continuous),
                Param::Resonance => ("Filter resonance", "RES", 0.2, Continuous),
                Param::Level => ("Output", "OUTPUT", 0.75, Continuous),
                Param::Sense => ("Sense", "SENSE", 0.5, Continuous),
                Param::Pan => ("Pan", "PAN", 0.5, Continuous),
                Param::NoiseLevel => ("Noise level", "LEVEL", 0.0, Continuous),
                Param::NoiseDecay => ("Noise decay", "DECAY", 0.3, Continuous),
                Param::NoiseTone => ("Noise tone", "TONE", 0.7, Continuous),
                Param::NoiseRes => ("Noise resonance", "RES", 0.2, Continuous),
                Param::NoiseType => (
                    "Noise filter",
                    "TYPE",
                    NOISE_HP as f32,
                    Choice(NOISE_TYPE_NAMES),
                ),
                Param::NoiseFm => ("Noise to FM", "N>FM", 0.0, Continuous),
                Param::LfoRate => ("LFO rate", "RATE", 0.45, Continuous),
                Param::LfoDepth => ("LFO depth", "DEPTH", 0.0, Continuous),
                Param::LfoWave => ("LFO wave", "WAVE", LFO_TRI as f32, Choice(LFO_WAVE_NAMES)),
                Param::LfoDest => (
                    "LFO destination",
                    "DEST",
                    LFO_PITCH as f32,
                    Choice(LFO_DEST_NAMES),
                ),
                Param::XmodAmount => ("Cross modulation", "XMOD", 0.0, Continuous),
                Param::XmodDest => (
                    "Cross modulation target",
                    "TO",
                    XMOD_CARRIERS as f32,
                    Choice(XMOD_DEST_NAMES),
                ),
                Param::Ring => ("Ring modulation", "RING", 0.0, Continuous),
                Param::Link => ("Link trigger", "LINK", 0.0, Choice(ON_OFF_NAMES)),
                Param::ReverbMix => ("Reverb amount", "AMOUNT", 0.0, Continuous),
                Param::ReverbDecay => ("Reverb decay", "DECAY", 0.45, Continuous),
                Param::ReverbTone => ("Reverb tone", "TONE", 0.6, Continuous),
                Param::ReverbPredelay => ("Reverb pre-delay", "PRE", 0.1, Continuous),
                _ => unreachable!("operator parameters are handled above"),
            }
        };
        ParamInfo {
            param: self,
            name,
            label,
            default,
            kind,
        }
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
        match self.info().kind {
            ParamKind::Continuous => v.clamp(0.0, 1.0),
            ParamKind::Choice(names) => v.round().clamp(0.0, (names.len() - 1) as f32),
        }
    }

    /// Controls centred on a neutral middle position.
    pub fn is_bipolar(self) -> bool {
        matches!(
            self,
            Param::Fine
                | Param::Pan
                | Param::SweepAmount
                | Param::Filter
                | Param::Op1Fine
                | Param::Op2Fine
                | Param::Op3Fine
                | Param::Op4Fine
        )
    }

    /// Human readable value for the panel read-out.
    pub fn display(self, v: f32) -> String {
        if let ParamKind::Choice(names) = self.info().kind {
            return names[self.sanitize(v) as usize].to_string();
        }
        if let Some((_, which)) = op_param(self) {
            return match which {
                OpParam::Ratio => format!("x{}", fmt_ratio(map::op_ratio(v))),
                OpParam::Fine => fmt_cents(map::op_fine_cents(v)),
                OpParam::Level => {
                    if v <= 0.0 {
                        "OFF".to_string()
                    } else {
                        format!("{:.0}", v * 99.0)
                    }
                }
                OpParam::Decay => fmt_secs(map::op_decay_seconds(v)),
            };
        }
        match self {
            Param::Tune => fmt_hz(map::tune_hz(v, 0.5)),
            Param::Fine => fmt_cents(map::fine_semitones(v) * 100.0),
            Param::Feedback => format!("{:.0}%", v * 100.0),
            Param::Decay => fmt_secs(map::decay_seconds(v)),
            Param::SweepAmount => {
                let oct = map::sweep_octaves(v);
                if oct.abs() < 0.005 {
                    "OFF".to_string()
                } else {
                    format!("{oct:+.2} oct")
                }
            }
            Param::SweepTime => fmt_secs(map::sweep_seconds(v)),
            Param::VelFm => format!("{:.0}%", v * 100.0),
            Param::Drive => format!("{:.0}%", v * 100.0),
            Param::Filter => {
                let f = map::filter_setting(v);
                match f {
                    map::FilterSetting::Open => "OPEN".to_string(),
                    map::FilterSetting::Lowpass(hz) => format!("LP {}", fmt_hz(hz)),
                    map::FilterSetting::Highpass(hz) => format!("HP {}", fmt_hz(hz)),
                }
            }
            Param::Resonance | Param::NoiseRes => format!("{:.0}%", v * 100.0),
            Param::Level => fmt_db(map::level_gain(v)),
            Param::Sense => format!("x{:.2}", map::sense_gain(v)),
            Param::Pan => {
                let p = (v - 0.5) * 200.0;
                if p.abs() < 1.0 {
                    "C".to_string()
                } else if p < 0.0 {
                    format!("L{:.0}", -p)
                } else {
                    format!("R{:.0}", p)
                }
            }
            Param::NoiseLevel => fmt_db(map::level_gain(v)),
            Param::NoiseDecay => fmt_secs(map::op_decay_seconds(v)),
            Param::NoiseTone => fmt_hz(map::noise_tone_hz(v)),
            Param::NoiseFm => format!("{:.0}%", v * 100.0),
            Param::LfoRate => fmt_hz(map::lfo_hz(v)),
            Param::LfoDepth => format!("{:.0}%", v * 100.0),
            Param::XmodAmount => format!("{:.2} rad", map::xmod_index(v) * core::f32::consts::TAU),
            Param::Ring => format!("{:.0}%", v * 100.0),
            Param::ReverbMix => fmt_db(map::reverb_gain(v)),
            Param::ReverbDecay => fmt_secs(map::reverb_seconds(v)),
            Param::ReverbTone => fmt_hz(map::reverb_tone_hz(v)),
            Param::ReverbPredelay => fmt_secs(map::reverb_predelay_seconds(v)),
            _ => format!("{v:.2}"),
        }
    }
}

const OP_RATIO_NAMES: [&str; OPS] = ["Op 1 ratio", "Op 2 ratio", "Op 3 ratio", "Op 4 ratio"];
const OP_FINE_NAMES: [&str; OPS] = ["Op 1 fine", "Op 2 fine", "Op 3 fine", "Op 4 fine"];
const OP_LEVEL_NAMES: [&str; OPS] = ["Op 1 level", "Op 2 level", "Op 3 level", "Op 4 level"];
const OP_DECAY_NAMES: [&str; OPS] = ["Op 1 decay", "Op 2 decay", "Op 3 decay", "Op 4 decay"];

/// Knob position that selects `ratio` (or the nearest entry of [`RATIOS`]).
pub const fn ratio_knob(ratio: f32) -> f32 {
    let mut best = 0;
    let mut i = 1;
    while i < RATIOS.len() {
        if (RATIOS[i] - ratio).abs() < (RATIOS[best] - ratio).abs() {
            best = i;
        }
        i += 1;
    }
    best as f32 / (RATIOS.len() - 1) as f32
}

fn fmt_ratio(r: f32) -> String {
    if (r - r.round()).abs() < 1e-4 {
        format!("{r:.0}")
    } else {
        format!("{r:.2}")
    }
}

fn fmt_cents(c: f32) -> String {
    if c.abs() < 0.5 {
        "0 ct".to_string()
    } else {
        format!("{c:+.0} ct")
    }
}

fn fmt_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else if hz >= 10.0 {
        format!("{:.0} Hz", hz)
    } else {
        format!("{:.2} Hz", hz)
    }
}

fn fmt_db(gain: f32) -> String {
    if gain <= 1e-4 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * gain.log10())
    }
}

fn fmt_secs(s: f32) -> String {
    if s >= 1.0 {
        format!("{:.2} s", s)
    } else if s >= 0.01 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{:.1} ms", s * 1000.0)
    }
}

/// Knob position → physical unit mappings.
pub mod map {
    use super::RATIOS;

    /// Lowest pitch at the bottom of the TUNE fader.
    pub const TUNE_MIN_HZ: f32 = 20.0;
    /// Span of the TUNE fader in octaves.
    pub const TUNE_OCTAVES: f32 = 7.0;
    /// Largest phase-modulation index of a full-level modulator, in cycles
    /// (≈ 13 radians, about where a DX7 tops out).
    pub const MAX_INDEX_CYCLES: f32 = 2.0;

    pub fn fine_semitones(fine: f32) -> f32 {
        (fine - 0.5) * 2.0
    }

    pub fn tune_octaves(tune: f32, fine: f32) -> f32 {
        TUNE_MIN_HZ.log2() + tune * TUNE_OCTAVES + fine_semitones(fine) / 12.0
    }

    pub fn tune_hz(tune: f32, fine: f32) -> f32 {
        tune_octaves(tune, fine).exp2()
    }

    /// The RATIO knob steps through [`RATIOS`].
    pub fn op_ratio(ratio: f32) -> f32 {
        let i = (ratio.clamp(0.0, 1.0) * (RATIOS.len() - 1) as f32).round() as usize;
        RATIOS[i]
    }

    /// Operator detune in cents, ±200 with fine control near the centre.
    pub fn op_fine_cents(fine: f32) -> f32 {
        let s = (fine - 0.5) * 2.0;
        200.0 * s * s.abs()
    }

    /// Operator output level: carrier amplitude, or the fraction of
    /// [`MAX_INDEX_CYCLES`] for a modulator. Squared for fine low-end control.
    pub fn op_level(level: f32) -> f32 {
        level * level
    }

    /// Operator envelope -60 dB decay time: 2 ms to 12 s.
    pub fn op_decay_seconds(decay: f32) -> f32 {
        0.002 * 6000f32.powf(decay)
    }

    /// Amp envelope -60 dB decay time: 10 ms to 15 s.
    pub fn decay_seconds(decay: f32) -> f32 {
        0.01 * 1500f32.powf(decay)
    }

    /// Operator 4 feedback, in cycles of phase per unit output. Past ~0.3
    /// the loop turns from sawtooth-like into noise, on purpose.
    pub fn feedback_cycles(fb: f32) -> f32 {
        0.8 * fb * fb
    }

    /// Pitch sweep depth in octaves, ±6. Positive starts above the note.
    pub fn sweep_octaves(amount: f32) -> f32 {
        let s = (amount - 0.5) * 2.0;
        6.0 * s * s.abs()
    }

    /// Pitch sweep time constant.
    pub fn sweep_seconds(time: f32) -> f32 {
        0.002 * 500f32.powf(time)
    }

    pub fn level_gain(level: f32) -> f32 {
        level * level
    }

    pub fn sense_gain(sense: f32) -> f32 {
        0.25 * 16f32.powf(sense)
    }

    pub fn drive_gain(drive: f32) -> f32 {
        1.0 + 15.0 * drive * drive
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum FilterSetting {
        Open,
        Lowpass(f32),
        Highpass(f32),
    }

    /// Width of the dead zone around the FILTER knob's centre.
    pub const FILTER_DEAD_ZONE: f32 = 0.02;

    /// The bipolar FILTER knob: turning left closes a lowpass from 20 kHz
    /// down to 40 Hz, turning right raises a highpass from 20 Hz to 8 kHz.
    pub fn filter_setting(filter: f32) -> FilterSetting {
        let s = (filter - 0.5) * 2.0;
        if s.abs() <= FILTER_DEAD_ZONE {
            FilterSetting::Open
        } else if s < 0.0 {
            let x = (-s - FILTER_DEAD_ZONE) / (1.0 - FILTER_DEAD_ZONE);
            FilterSetting::Lowpass(20_000.0 * (40.0f32 / 20_000.0).powf(x))
        } else {
            let x = (s - FILTER_DEAD_ZONE) / (1.0 - FILTER_DEAD_ZONE);
            FilterSetting::Highpass(20.0 * 400f32.powf(x))
        }
    }

    /// Filter Q from the RES knob.
    pub fn resonance_q(res: f32) -> f32 {
        0.5 + 11.5 * res * res
    }

    pub fn noise_tone_hz(tone: f32) -> f32 {
        60.0 * 300f32.powf(tone)
    }

    /// Noise phase modulation of the carriers, in cycles.
    pub fn noise_fm_cycles(amount: f32) -> f32 {
        0.5 * amount * amount
    }

    pub fn lfo_hz(rate: f32) -> f32 {
        0.2 * 400f32.powf(rate)
    }

    /// LFO depth on pitch, in octaves.
    pub fn lfo_pitch_octaves(depth: f32) -> f32 {
        2.0 * depth * depth
    }

    /// Cross modulation index, in cycles per unit of the other channel.
    pub fn xmod_index(amount: f32) -> f32 {
        1.5 * amount * amount
    }

    pub fn reverb_gain(amount: f32) -> f32 {
        amount * amount
    }

    /// Approximate RT60 of the plate.
    pub fn reverb_seconds(decay: f32) -> f32 {
        0.3 * 40f32.powf(decay)
    }

    /// Damping lowpass cutoff (input bandwidth and tank damping).
    pub fn reverb_tone_hz(tone: f32) -> f32 {
        1500.0 * 12f32.powf(tone)
    }

    pub fn reverb_predelay_seconds(predelay: f32) -> f32 {
        0.15 * predelay
    }
}

/// A complete voice patch: one value per [`Param`], indexed by `Param::index`.
pub type VoicePatch = [f32; PARAM_COUNT];

pub fn default_patch() -> VoicePatch {
    let mut p = [0.0; PARAM_COUNT];
    for param in ALL_PARAMS {
        p[param.index()] = param.default_value();
    }
    p
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
    fn op_params_map_both_ways() {
        for op in 0..OPS {
            for which in OpParam::ALL {
                assert_eq!(op_param(which.of(op)), Some((op, which)));
            }
        }
        assert_eq!(op_param(Param::Tune), None);
        assert_eq!(op_param(Param::NoiseLevel), None);
        assert_eq!(OpParam::Decay.of(3), Param::Op4Decay);
    }

    #[test]
    fn algorithms_only_modulate_downwards() {
        for (n, a) in ALGORITHMS.iter().enumerate() {
            assert!(a.carriers & 1 != 0, "algorithm {n}: op 1 must be heard");
            for to in 0..OPS {
                for from in 0..OPS {
                    if a.modulates(from, to) {
                        assert!(from > to, "algorithm {n}: {from} -> {to}");
                    }
                }
            }
        }
        assert_eq!(ALGORITHMS.len(), ALGORITHM_NAMES.len());
    }

    #[test]
    fn ratios_are_sorted_and_reachable() {
        assert!(RATIOS.windows(2).all(|w| w[0] < w[1]));
        for &r in &RATIOS {
            assert_eq!(map::op_ratio(ratio_knob(r)), r);
        }
    }

    #[test]
    fn sanitize_quantizes_choices() {
        assert_eq!(Param::Algorithm.sanitize(2.4), 2.0);
        assert_eq!(Param::Algorithm.sanitize(99.0), 7.0);
        assert_eq!(Param::Tune.sanitize(1.5), 1.0);
        assert_eq!(Param::Tune.sanitize(f32::NAN), Param::Tune.default_value());
    }

    #[test]
    fn mapping_ranges() {
        assert!((map::tune_hz(0.0, 0.5) - 20.0).abs() < 0.01);
        assert!((map::tune_hz(1.0, 0.5) - 2560.0).abs() < 0.5);
        assert!((map::op_decay_seconds(1.0) - 12.0).abs() < 1e-3);
        assert!((map::decay_seconds(1.0) - 15.0).abs() < 1e-3);
        assert_eq!(map::sweep_octaves(0.5), 0.0);
        assert!((map::sweep_octaves(1.0) - 6.0).abs() < 1e-6);
        assert_eq!(map::filter_setting(0.5), map::FilterSetting::Open);
        assert!(
            matches!(map::filter_setting(0.0), map::FilterSetting::Lowpass(hz) if (hz - 40.0).abs() < 0.1)
        );
        assert!(
            matches!(map::filter_setting(1.0), map::FilterSetting::Highpass(hz) if (hz - 8000.0).abs() < 1.0)
        );
    }

    #[test]
    fn display_is_sane() {
        assert_eq!(Param::Algorithm.display(3.0), "4");
        assert_eq!(Param::Pan.display(0.5), "C");
        assert_eq!(Param::Tune.display(1.0), "2.56 kHz");
        assert_eq!(Param::Op2Ratio.display(ratio_knob(2.76)), "x2.76");
        assert_eq!(Param::Op2Ratio.display(ratio_knob(3.0)), "x3");
        assert_eq!(Param::Filter.display(0.5), "OPEN");
        assert_eq!(Param::SweepAmount.display(0.5), "OFF");
    }
}
