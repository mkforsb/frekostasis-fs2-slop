//! One Frekussion channel: a four-operator FM voice with a dedicated noise
//! oscillator, built for percussion.
//!
//! ```text
//!             ┌──────── pitch: TUNE + SWEEP env + LFO ────────┐
//!             ▼                                               │
//!  other ch ─XMOD─►  OP4 ⟲ ─► OP3 ─► OP2 ─► OP1   (routing set  │
//!                    each with RATIO, LEVEL, DECAY  by ALGO)   │
//!  NOISE ─► SVF ─N>FM─►──────────────── carriers ─┐            │
//!    │                                            ▼            │
//!    │                          other ch ─RING─► × AMP env ─► XMOD out
//!    │                                            │
//!    └─► × NOISE env ──────────────────────────►  + ─► DRIVE ─► FILTER ─► OUTPUT
//! ```
//!
//! Operators are sine oscillators whose phases are reset on every hit so each
//! hit starts the same way. Each operator has its own exponential decay: a
//! short modulator decay gives the bright FM "click" of the attack, a long
//! one keeps the tone clangorous. The amp envelope shapes the whole FM
//! voice; the noise has its own envelope and does not pass through it.

use crate::params::{
    ALGORITHMS, Algorithm, LFO_AMP, LFO_FM, LFO_PITCH, LFO_SAW, LFO_SH, LFO_SQR, NOISE_BP,
    NOISE_HP, OPS, OpParam, PARAM_COUNT, Param, VoicePatch, XMOD_ALL, XMOD_CARRIERS,
    XMOD_MODULATORS, default_patch, map,
};

/// Control-rate period, in (oversampled) samples. Envelope coefficients,
/// filters and smoothed parameters update this often; pitch every sample.
const CONTROL: u32 = 16;
/// Parameter de-zippering.
const SMOOTH_TAU_S: f32 = 0.004;
/// Amp envelope attack: short enough to keep the FM click, long enough not
/// to be a raw step.
const ATTACK_S: f32 = 0.000_3;
/// Time constant that fades out the old sound's last value on a retrigger.
const DECLICK_TAU_S: f32 = 0.001;
/// Ring modulation make-up gain (two enveloped signals multiply to less).
const RING_GAIN: f32 = 2.0;
/// White noise is quieter than a full-scale sine; this evens them out.
const NOISE_GAIN: f32 = 2.0;
/// Harder hits ring longer: decay scale at velocity 0 (1 at full velocity).
const DECAY_VEL_MIN: f32 = 0.7;

/// Which parameters are de-zippered (continuous ones).
const SMOOTHED: [bool; PARAM_COUNT] = {
    let mut s = [true; PARAM_COUNT];
    s[Param::Algorithm as usize] = false;
    s[Param::NoiseType as usize] = false;
    s[Param::LfoWave as usize] = false;
    s[Param::LfoDest as usize] = false;
    s[Param::XmodDest as usize] = false;
    s[Param::Link as usize] = false;
    // Stepped knobs: gliding through the intermediate ratios would chirp.
    s[Param::Op1Ratio as usize] = false;
    s[Param::Op2Ratio as usize] = false;
    s[Param::Op3Ratio as usize] = false;
    s[Param::Op4Ratio as usize] = false;
    s
};

/// xorshift32 white noise.
#[derive(Clone, Debug)]
pub struct Noise {
    state: u32,
}

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    /// Uniform white noise in `[-1, 1)`.
    #[inline]
    pub fn sample(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }
}

/// `sin(2π·x)` for a phase `x` in cycles (any value), accurate to ~1e-7.
#[inline]
pub fn sin_cycles(x: f32) -> f32 {
    // Fold into [-¼, ¼] cycle, where the Taylor series converges quickly.
    let t = x - x.floor() - 0.5;
    let t = if t > 0.25 {
        0.5 - t
    } else if t < -0.25 {
        -0.5 - t
    } else {
        t
    };
    let z = t * core::f32::consts::TAU;
    let z2 = z * z;
    let p = 1.0
        + z2 * (-1.0 / 6.0
            + z2 * (1.0 / 120.0
                + z2 * (-1.0 / 5040.0 + z2 * (1.0 / 362_880.0 + z2 * (-1.0 / 39_916_800.0)))));
    // sin(2π(t + ½)) = -sin(2πt)
    -z * p
}

/// Rational tanh approximation, accurate to ~0.2% and exact at the clamp.
#[inline]
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// Precomputed coefficients for [`Svf::tick`].
#[derive(Clone, Copy, Debug)]
pub struct SvfCoeffs {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl SvfCoeffs {
    pub fn new(fc_hz: f32, q: f32, fs: f32) -> Self {
        let g = (core::f32::consts::PI * fc_hz.clamp(10.0, 0.45 * fs) / fs).tan();
        let k = 1.0 / q.max(0.3);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            k,
            a1,
            a2,
            a3: g * a2,
        }
    }
}

impl Default for SvfCoeffs {
    fn default() -> Self {
        Self::new(1000.0, 0.707, 96_000.0)
    }
}

/// Zero-delay-feedback state variable filter (Simper), all three outputs.
#[derive(Clone, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Returns `(lowpass, bandpass, highpass)`.
    #[inline]
    pub fn tick(&mut self, x: f32, c: &SvfCoeffs) -> (f32, f32, f32) {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = flush(2.0 * v1 - self.ic1);
        self.ic2 = flush(2.0 * v2 - self.ic2);
        (v2, v1, x - c.k * v1 - v2)
    }
}

#[inline]
fn one_pole_coeff(tau_s: f32, fs: f32) -> f32 {
    1.0 - (-1.0 / (tau_s * fs)).exp()
}

/// Per-sample multiplier that decays by 60 dB in `seconds`.
#[inline]
fn decay_coeff(seconds: f32, fs: f32) -> f32 {
    (-6.907_755 / (seconds.max(1e-4) * fs)).exp()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FilterMode {
    Low,
    High,
}

/// Everything derived from the knobs at control rate.
#[derive(Clone, Copy, Debug)]
struct Controls {
    algo: Algorithm,
    /// Frequency multiplier (ratio × fine) per operator.
    mult: [f32; OPS],
    /// Output amplitude per operator (carrier gain or index fraction).
    level: [f32; OPS],
    k_op: [f32; OPS],
    k_amp: f32,
    k_sweep: f32,
    k_noise: f32,
    feedback: f32,
    base_oct: f32,
    sweep_oct: f32,
    carrier_norm: f32,
    noise_gain: f32,
    noise_fm: f32,
    noise_type: u32,
    noise_svf: SvfCoeffs,
    filter_mode: FilterMode,
    filter_svf: SvfCoeffs,
    drive_gain: f32,
    drive_mix: f32,
    lfo_inc: f32,
    lfo_depth: f32,
    lfo_wave: u32,
    lfo_dest: u32,
    xmod: f32,
    xmod_dest: u32,
    ring: f32,
    out_gain: f32,
}

#[derive(Clone, Debug)]
pub struct Voice {
    fs: f32,
    target: VoicePatch,
    smooth: VoicePatch,
    k_smooth: f32,
    ctl: Controls,
    ctl_left: u32,

    phase: [f32; OPS],
    op_env: [f32; OPS],
    /// Operator 4's last two outputs, averaged for the feedback path.
    fb: [f32; 2],
    amp: f32,
    attack_left: u32,
    attack_step: f32,
    attack_len: u32,
    sweep: f32,
    noise_env: f32,
    velocity: f32,
    /// Index scale from velocity and VEL>FM, fixed per hit.
    vel_index: f32,

    noise: Noise,
    noise_svf: Svf,
    filter: Svf,

    lfo_phase: f32,
    lfo_sh: f32,

    /// This channel's enveloped FM signal, which the other channel uses for
    /// cross and ring modulation.
    xsig: f32,
    /// Last FM output before a retrigger, fading out.
    declick: f32,
    k_declick: f32,
    last_fm: f32,
}

impl Voice {
    /// `fs` is the rate `tick` is called at (the oversampled rate).
    pub fn new(fs: f32, seed: u32) -> Self {
        let patch = default_patch();
        let mut v = Self {
            fs,
            target: patch,
            smooth: patch,
            k_smooth: one_pole_coeff(SMOOTH_TAU_S / CONTROL as f32, fs),
            ctl_left: 0,
            ctl: Controls {
                algo: ALGORITHMS[0],
                mult: [1.0; OPS],
                level: [0.0; OPS],
                k_op: [0.0; OPS],
                k_amp: 0.0,
                k_sweep: 0.0,
                k_noise: 0.0,
                feedback: 0.0,
                base_oct: 0.0,
                sweep_oct: 0.0,
                carrier_norm: 1.0,
                noise_gain: 0.0,
                noise_fm: 0.0,
                noise_type: 0,
                noise_svf: SvfCoeffs::default(),
                filter_mode: FilterMode::Low,
                filter_svf: SvfCoeffs::default(),
                drive_gain: 1.0,
                drive_mix: 0.0,
                lfo_inc: 0.0,
                lfo_depth: 0.0,
                lfo_wave: 0,
                lfo_dest: 0,
                xmod: 0.0,
                xmod_dest: 0,
                ring: 0.0,
                out_gain: 0.0,
            },
            phase: [0.0; OPS],
            op_env: [0.0; OPS],
            fb: [0.0; 2],
            amp: 0.0,
            attack_left: 0,
            attack_step: 0.0,
            attack_len: ((ATTACK_S * fs) as u32).max(1),
            sweep: 0.0,
            noise_env: 0.0,
            velocity: 1.0,
            vel_index: 1.0,
            noise: Noise::new(seed.wrapping_mul(0x9E37_79B9) ^ 0x1234_5678),
            noise_svf: Svf::default(),
            filter: Svf::default(),
            lfo_phase: 0.0,
            lfo_sh: 0.0,
            xsig: 0.0,
            declick: 0.0,
            k_declick: decay_coeff(DECLICK_TAU_S * 6.9, fs),
            last_fm: 0.0,
        };
        v.update_controls();
        v
    }

    pub fn set_param(&mut self, param: Param, value: f32) {
        let v = param.sanitize(value);
        self.target[param.index()] = v;
        if !SMOOTHED[param.index()] {
            self.smooth[param.index()] = v;
        }
    }

    pub fn param(&self, param: Param) -> f32 {
        self.target[param.index()]
    }

    /// The de-zippered value the voice is currently using.
    pub fn smoothed(&self, param: Param) -> f32 {
        self.smooth[param.index()]
    }

    /// Jump all smoothed parameters to their targets (e.g. after loading a patch
    /// into a silent engine, or so a new patch starts exactly on a hit).
    pub fn snap_params(&mut self) {
        self.smooth = self.target;
        self.update_controls();
    }

    /// Whether this channel fires when the other channel is triggered.
    pub fn linked(&self) -> bool {
        self.target[Param::Link.index()] >= 0.5
    }

    /// Fire the voice. `velocity` is the raw hit strength `0..=1`; SENSE
    /// scales it like a trigger input's preamp.
    pub fn trigger(&mut self, velocity: f32) {
        let gain = map::sense_gain(self.target[Param::Sense.index()]);
        self.velocity = (velocity.clamp(0.0, 1.0) * gain).clamp(0.02, 1.0);
        let vel_fm = self.smooth[Param::VelFm.index()];
        self.vel_index = 1.0 - vel_fm * (1.0 - self.velocity);
        // Velocity changes decay times, so refresh the coefficients now.
        self.update_controls();

        // `last_fm` already includes any declick still fading out.
        self.declick = self.last_fm;
        self.phase = [0.0; OPS];
        self.fb = [0.0; 2];
        self.op_env = [1.0; OPS];
        self.attack_left = self.attack_len;
        self.attack_step = (self.velocity - self.amp) / self.attack_len as f32;
        self.sweep = 1.0;
        self.noise_env = self.velocity;
        self.lfo_phase = 0.0;
        self.lfo_sh = self.noise.sample();
    }

    /// Current amp envelope level (for metering).
    pub fn envelope(&self) -> f32 {
        self.amp
    }

    /// The signal this voice offers the other channel for XMOD and RING.
    #[inline]
    pub fn xsig(&self) -> f32 {
        self.xsig
    }

    fn update_controls(&mut self) {
        for ((cur, &target), &smoothed) in self.smooth.iter_mut().zip(&self.target).zip(&SMOOTHED) {
            if smoothed {
                *cur += (target - *cur) * self.k_smooth;
            }
        }
        let p = &self.smooth;
        let fs = self.fs;
        let get = |param: Param| p[param.index()];
        let algo = ALGORITHMS[(get(Param::Algorithm) as usize).min(ALGORITHMS.len() - 1)];
        let decay_vel = DECAY_VEL_MIN + (1.0 - DECAY_VEL_MIN) * self.velocity;

        let mut mult = [1.0; OPS];
        let mut level = [0.0; OPS];
        let mut k_op = [0.0; OPS];
        for op in 0..OPS {
            let ratio = map::op_ratio(get(OpParam::Ratio.of(op)));
            let cents = map::op_fine_cents(get(OpParam::Fine.of(op)));
            mult[op] = ratio * (cents / 1200.0).exp2();
            level[op] = map::op_level(get(OpParam::Level.of(op)));
            k_op[op] = decay_coeff(
                map::op_decay_seconds(get(OpParam::Decay.of(op))) * decay_vel,
                fs,
            );
        }

        let (filter_mode, fc, q) = match map::filter_setting(get(Param::Filter)) {
            map::FilterSetting::Open => (FilterMode::Low, 0.45 * fs, 0.707),
            map::FilterSetting::Lowpass(hz) => {
                (FilterMode::Low, hz, map::resonance_q(get(Param::Resonance)))
            }
            map::FilterSetting::Highpass(hz) => (
                FilterMode::High,
                hz,
                map::resonance_q(get(Param::Resonance)),
            ),
        };
        let drive = get(Param::Drive);

        self.ctl = Controls {
            algo,
            mult,
            level,
            k_op,
            k_amp: decay_coeff(map::decay_seconds(get(Param::Decay)) * decay_vel, fs),
            k_sweep: (-1.0 / (map::sweep_seconds(get(Param::SweepTime)) * fs)).exp(),
            k_noise: decay_coeff(
                map::op_decay_seconds(get(Param::NoiseDecay)) * decay_vel,
                fs,
            ),
            feedback: map::feedback_cycles(get(Param::Feedback)),
            base_oct: map::tune_octaves(get(Param::Tune), get(Param::Fine)),
            sweep_oct: map::sweep_octaves(get(Param::SweepAmount)),
            carrier_norm: 1.0 / (algo.carrier_count() as f32).sqrt(),
            noise_gain: NOISE_GAIN * map::level_gain(get(Param::NoiseLevel)),
            noise_fm: map::noise_fm_cycles(get(Param::NoiseFm)),
            noise_type: p[Param::NoiseType.index()] as u32,
            noise_svf: SvfCoeffs::new(
                map::noise_tone_hz(get(Param::NoiseTone)),
                map::resonance_q(get(Param::NoiseRes)),
                fs,
            ),
            filter_mode,
            filter_svf: SvfCoeffs::new(fc, q, fs),
            drive_gain: map::drive_gain(drive),
            drive_mix: (drive * 5.0).min(1.0),
            lfo_inc: map::lfo_hz(get(Param::LfoRate)) / fs,
            lfo_depth: get(Param::LfoDepth),
            lfo_wave: p[Param::LfoWave.index()] as u32,
            lfo_dest: p[Param::LfoDest.index()] as u32,
            xmod: map::xmod_index(get(Param::XmodAmount)),
            xmod_dest: p[Param::XmodDest.index()] as u32,
            ring: get(Param::Ring),
            out_gain: map::level_gain(get(Param::Level)),
        };
        self.ctl_left = CONTROL;
    }

    #[inline]
    fn lfo(&mut self) -> f32 {
        let c = &self.ctl;
        self.lfo_phase += c.lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= self.lfo_phase.floor();
            self.lfo_sh = self.noise.sample();
        }
        let p = self.lfo_phase;
        match c.lfo_wave {
            // Starts at zero and rises, like a sine.
            LFO_SQR => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LFO_SAW => 1.0 - 2.0 * p,
            LFO_SH => self.lfo_sh,
            _ => {
                if p < 0.25 {
                    4.0 * p
                } else if p < 0.75 {
                    2.0 - 4.0 * p
                } else {
                    4.0 * p - 4.0
                }
            }
        }
    }

    /// Render one sample. `other` is the other channel's [`Voice::xsig`]
    /// from the previous sample.
    #[inline]
    pub fn tick(&mut self, other: f32) -> f32 {
        if self.ctl_left == 0 {
            self.update_controls();
        }
        self.ctl_left -= 1;
        let fs = self.fs;
        let lfo = self.lfo();
        let c = &self.ctl;

        // --- Envelopes ---------------------------------------------------
        if self.attack_left > 0 {
            self.attack_left -= 1;
            self.amp += self.attack_step;
        } else {
            self.amp *= c.k_amp;
        }
        for (e, k) in self.op_env.iter_mut().zip(c.k_op) {
            *e *= k;
        }
        self.sweep *= c.k_sweep;
        self.noise_env *= c.k_noise;
        self.declick *= self.k_declick;

        // --- LFO routing -------------------------------------------------
        let depth = c.lfo_depth;
        let (lfo_oct, lfo_index, lfo_amp) = match c.lfo_dest {
            LFO_PITCH => (lfo * map::lfo_pitch_octaves(depth), 1.0, 1.0),
            LFO_FM => (0.0, (1.0 + depth * lfo).max(0.0), 1.0),
            LFO_AMP => (0.0, 1.0, 1.0 - depth * (0.5 - 0.5 * lfo)),
            _ => (0.0, 1.0, 1.0),
        };

        // --- Noise -------------------------------------------------------
        let (lp, bp, hp) = self.noise_svf.tick(self.noise.sample(), &c.noise_svf);
        let colored = match c.noise_type {
            // Keep the band's loudness independent of its width.
            NOISE_BP => bp * c.noise_svf.k.sqrt(),
            NOISE_HP => hp,
            _ => lp,
        };
        let noise_env = self.noise_env;
        let noise_fm = colored * noise_env * c.noise_fm;

        // --- Operators ---------------------------------------------------
        let pitch_oct = c.base_oct + c.sweep_oct * self.sweep + lfo_oct;
        let f0 = pitch_oct.exp2();
        let max_inc = 0.45;
        let index = map::MAX_INDEX_CYCLES * self.vel_index * lfo_index;
        let x_in = other * c.xmod;
        let algo = c.algo;
        let mut out = [0.0f32; OPS];
        let mut fm = 0.0;
        for op in (0..OPS).rev() {
            let carrier = algo.is_carrier(op);
            let mut pm = 0.0;
            for (from, o) in out.iter().enumerate().skip(op + 1) {
                if algo.modulates(from, op) {
                    pm += o;
                }
            }
            if op == OPS - 1 {
                pm += c.feedback * 0.5 * (self.fb[0] + self.fb[1]);
            }
            let x_here = match c.xmod_dest {
                XMOD_CARRIERS => carrier,
                XMOD_MODULATORS => !carrier,
                XMOD_ALL => true,
                _ => false,
            };
            if x_here {
                pm += x_in;
            }
            if carrier {
                pm += noise_fm;
            }
            let s = sin_cycles(self.phase[op] + pm);
            self.phase[op] += (f0 * c.mult[op] / fs).min(max_inc);
            if self.phase[op] >= 1.0 {
                self.phase[op] -= self.phase[op].floor();
            }
            let a = s * c.level[op] * self.op_env[op];
            if op == OPS - 1 {
                self.fb = [a, self.fb[0]];
            }
            if carrier {
                fm += a;
            } else {
                out[op] = a * index;
            }
        }
        fm *= c.carrier_norm;

        // --- Amp, ring, noise, drive, filter -----------------------------
        let voiced = fm * self.amp;
        self.xsig = voiced;
        let ringed = voiced + c.ring * (voiced * other * RING_GAIN - voiced);
        self.last_fm = ringed + self.declick;
        let mut x = (self.last_fm + colored * noise_env * c.noise_gain) * lfo_amp;
        if c.drive_mix > 0.0 {
            let wet = fast_tanh(c.drive_gain * x) / c.drive_gain.sqrt();
            x += (wet - x) * c.drive_mix;
        }
        let (lp, _, hp) = self.filter.tick(x, &c.filter_svf);
        let y = match c.filter_mode {
            FilterMode::Low => lp,
            FilterMode::High => hp,
        };

        if self.amp < 1e-7 && self.attack_left == 0 {
            self.amp = 0.0;
        }
        if self.noise_env < 1e-7 {
            self.noise_env = 0.0;
        }
        if self.declick.abs() < 1e-7 {
            self.declick = 0.0;
        }
        for e in self.op_env.iter_mut() {
            if *e < 1e-7 {
                *e = 0.0;
            }
        }
        y * c.out_gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_centered() {
        let mut n = Noise::new(1);
        let mut sum = 0.0f64;
        for _ in 0..100_000 {
            let x = n.sample();
            assert!((-1.0..1.0).contains(&x));
            sum += x as f64;
        }
        assert!((sum / 100_000.0).abs() < 0.01);
    }

    #[test]
    fn sine_is_accurate() {
        for i in -2000..=2000 {
            let x = i as f32 * 0.00173;
            let want = (x * core::f32::consts::TAU).sin();
            assert!((sin_cycles(x) - want).abs() < 2e-6, "x={x}");
        }
    }

    #[test]
    fn fast_tanh_close() {
        for i in -40..=40 {
            let x = i as f32 * 0.1;
            assert!((fast_tanh(x) - x.tanh()).abs() < 0.025, "x={x}");
        }
    }

    #[test]
    fn svf_outputs_split_the_spectrum() {
        let fs = 96_000.0;
        let c = SvfCoeffs::new(1000.0, 0.707, fs);
        let gain = |hz: f32, pick: fn((f32, f32, f32)) -> f32| {
            let mut f = Svf::default();
            let mut peak = 0.0f32;
            for n in 0..(fs as usize / 5) {
                let y = pick(f.tick(sin_cycles(hz * n as f32 / fs), &c));
                if n > fs as usize / 10 {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        assert!(gain(100.0, |o| o.0) > 0.9);
        assert!(gain(10_000.0, |o| o.0) < 0.05);
        assert!(gain(10_000.0, |o| o.2) > 0.9);
        assert!(gain(100.0, |o| o.2) < 0.05);
    }

    #[test]
    fn velocity_scales_the_modulation_index() {
        let mut v = Voice::new(96_000.0, 1);
        v.set_param(Param::VelFm, 1.0);
        v.set_param(Param::Sense, 0.5);
        v.snap_params();
        v.trigger(1.0);
        let hard = v.vel_index;
        v.trigger(0.2);
        assert!(v.vel_index < hard * 0.5);
    }
}
