//! Ping-pong space echo.
//!
//! Two delay lines cross-feed each other: the input goes into the left one,
//! each left repeat is fed to the right, and each right repeat back to the
//! left through the feedback gain. The right line's time is the left's
//! times a SPACING ratio. With an irrational ratio (GOLDEN, SILVER) the
//! repeats never fall back into step, so the ping-pong spreads into an
//! irregular, drifting cloud.
//!
//! Every pass through the loop goes through the MODE's processor:
//!
//! * **TAPE**: lowpass (TONE), a gentle highpass and tape saturation, with
//!   wow and flutter on both heads. Past 100% feedback it runs away into
//!   soft-clipped self-oscillation, like a pushed Space Echo.
//! * **BAND**: a band-pass at TONE on every repeat: the echoes thin out into
//!   a ringing, radio-like band.
//! * **SPIRAL**: a frequency shifter (TONE: ±30 Hz). Every repeat is shifted
//!   again, so the echoes climb or fall in an endless barber-pole spiral;
//!   shifting by Hz rather than a ratio also detunes harmonics against each
//!   other, which sounds suitably alien.
//! * **DIFFUSE**: a chain of allpasses (TONE: amount) smears each repeat a
//!   little more, so the echoes dissolve into reverb-like clouds.
//!
//! Delay times glide like a tape motor when changed.

use crate::util::{DelayLine, Svf, SvfCoeffs, fast_tanh, one_pole_coeff, sin_turns};

use crate::params::{
    ECHO_BAND, ECHO_DIFFUSE, ECHO_SPIRAL, ECHO_TAPE, Param, Patch, SPACING_RATIOS, SYNC_DIVISIONS,
    map,
};

const TIME_GLIDE_SECONDS: f32 = 0.25;
const ON_OFF_SECONDS: f32 = 0.03;
const WOW_HZ: f32 = 0.55;
const WOW_DEPTH: f32 = 0.0025;
const FLUTTER_HZ: f32 = 6.3;
const FLUTTER_DEPTH: f32 = 0.0007;
/// Allpass lengths of the DIFFUSE smear, seconds.
const DIFFUSER_SECONDS: [f32; 3] = [0.0071, 0.0113, 0.0173];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EchoSettings {
    pub on: bool,
    pub left_seconds: f32,
    pub ratio: f32,
    pub mode: u32,
    /// Raw TONE knob; its meaning depends on the mode.
    pub tone: f32,
    pub feedback: f32,
    pub level: f32,
}

impl EchoSettings {
    pub fn from_patch(p: &Patch) -> Self {
        let v = |param: Param| param.sanitize(p[param.index()]);
        let time = if v(Param::EchoSync) > 0.5 {
            map::step_seconds(v(Param::StepTime))
                * SYNC_DIVISIONS[map::sync_index(v(Param::EchoTime))]
        } else {
            map::echo_seconds(v(Param::EchoTime))
        };
        Self {
            on: v(Param::EchoOn) > 0.5,
            left_seconds: time.clamp(0.005, map::ECHO_MAX_S),
            ratio: SPACING_RATIOS[v(Param::EchoSpacing) as usize],
            mode: v(Param::EchoMode) as u32,
            tone: v(Param::EchoTone),
            feedback: map::echo_feedback(v(Param::EchoFeedback)),
            level: v(Param::EchoLevel),
        }
    }
}

/// Second-order allpass section in `z⁻²`, as used by the Hilbert pair.
#[derive(Clone, Copy, Debug, Default)]
struct Ap2 {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Ap2 {
    #[inline]
    fn process(&mut self, x: f32, c: f32) -> f32 {
        let y = c * (x + self.y2) - self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = if y.abs() < 1e-20 { 0.0 } else { y };
        y
    }
}

/// Two allpass chains whose outputs are 90° apart over most of the audio
/// band (Olli Niemitalo's IIR Hilbert transformer).
#[derive(Clone, Copy, Debug, Default)]
pub struct Hilbert {
    a: [Ap2; 4],
    b: [Ap2; 4],
    delayed: f32,
}

const HILBERT_A: [f32; 4] = [0.692_387_8, 0.936_065_4, 0.988_229_5, 0.998_748_8];
const HILBERT_B: [f32; 4] = [0.402_192_1, 0.856_171_1, 0.972_290_9, 0.995_288_5];

impl Hilbert {
    /// `(in-phase, quadrature)`.
    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let mut a = x;
        for (s, c) in self.a.iter_mut().zip(HILBERT_A) {
            a = s.process(a, c * c);
        }
        let mut b = x;
        for (s, c) in self.b.iter_mut().zip(HILBERT_B) {
            b = s.process(b, c * c);
        }
        let i = self.delayed;
        self.delayed = a;
        (i, b)
    }
}

/// Single-sideband frequency shifter.
#[derive(Clone, Copy, Debug, Default)]
pub struct FreqShifter {
    hilbert: Hilbert,
    phase: f32,
}

impl FreqShifter {
    #[inline]
    pub fn process(&mut self, x: f32, hz: f32, sample_rate: f32) -> f32 {
        let (i, q) = self.hilbert.process(x);
        self.phase += hz / sample_rate;
        self.phase -= self.phase.floor();
        i * sin_turns(self.phase + 0.25) + q * sin_turns(self.phase)
    }
}

/// Schroeder allpass on a plain ring buffer.
#[derive(Clone, Debug)]
struct Diffuser {
    buf: Vec<f32>,
    pos: usize,
}

impl Diffuser {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32, g: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let v = x - g * delayed;
        self.buf[self.pos] = if v.abs() < 1e-20 { 0.0 } else { v };
        self.pos = (self.pos + 1) % self.buf.len();
        delayed + g * v
    }

    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

/// Per-side state of the feedback processor.
#[derive(Clone, Debug)]
struct Side {
    lp: f32,
    hp: f32,
    svf: Svf,
    shifter: FreqShifter,
    diffusers: [Diffuser; 3],
}

impl Side {
    fn new(sample_rate: f32, stagger: f32) -> Self {
        Self {
            lp: 0.0,
            hp: 0.0,
            svf: Svf::default(),
            shifter: FreqShifter::default(),
            diffusers: DIFFUSER_SECONDS
                .map(|s| Diffuser::new((s * stagger * sample_rate) as usize)),
        }
    }

    fn clear(&mut self) {
        self.lp = 0.0;
        self.hp = 0.0;
        self.svf.reset();
        self.shifter = FreqShifter::default();
        self.diffusers.iter_mut().for_each(Diffuser::clear);
    }
}

/// Coefficients for the current block.
#[derive(Clone, Copy, Debug)]
struct Coeffs {
    mode: u32,
    lp: f32,
    hp: f32,
    band: SvfCoeffs,
    band_gain: f32,
    shift_hz: f32,
    diffusion: f32,
}

pub struct Echo {
    sample_rate: f32,
    lines: [DelayLine; 2],
    sides: [Side; 2],
    /// Smoothed delay times, seconds.
    time: [f32; 2],
    k_time: f32,
    /// On/off fade.
    active: f32,
    k_active: f32,
    wow: f32,
    flutter: f32,
    coeffs: Coeffs,
    settings: EchoSettings,
    /// Input has been silent and the lines drained: nothing to do.
    idle: bool,
}

impl Echo {
    pub fn new(sample_rate: f32) -> Self {
        let max = (map::ECHO_MAX_S * (1.0 + 2.0 * (WOW_DEPTH + FLUTTER_DEPTH)) * sample_rate)
            as usize
            + 8;
        let settings = EchoSettings::from_patch(&crate::params::default_patch());
        let mut echo = Self {
            sample_rate,
            lines: [DelayLine::new(max), DelayLine::new(max)],
            sides: [Side::new(sample_rate, 1.0), Side::new(sample_rate, 1.23)],
            time: [
                settings.left_seconds,
                settings.left_seconds * settings.ratio,
            ],
            k_time: one_pole_coeff(TIME_GLIDE_SECONDS, sample_rate),
            active: 0.0,
            k_active: one_pole_coeff(ON_OFF_SECONDS, sample_rate),
            wow: 0.0,
            flutter: 0.37,
            coeffs: Coeffs {
                mode: ECHO_TAPE,
                lp: 0.5,
                hp: 0.0,
                band: SvfCoeffs::new(1000.0, 1.0, sample_rate),
                band_gain: 1.0,
                shift_hz: 0.0,
                diffusion: 0.0,
            },
            settings,
            idle: true,
        };
        echo.prepare(&settings);
        echo
    }

    /// Take the settings for the next block.
    pub fn prepare(&mut self, s: &EchoSettings) {
        let sr = self.sample_rate;
        let lp_hz = match s.mode {
            ECHO_TAPE => map::echo_tape_cutoff_hz(s.tone),
            ECHO_SPIRAL => 9_000.0,
            _ => 6_000.0,
        };
        let q = 1.6;
        self.coeffs = Coeffs {
            mode: s.mode,
            lp: 1.0 - (-core::f32::consts::TAU * lp_hz.min(0.45 * sr) / sr).exp(),
            hp: 1.0 - (-core::f32::consts::TAU * 45.0 / sr).exp(),
            band: SvfCoeffs::new(map::echo_band_hz(s.tone), q, sr),
            band_gain: 1.0 / q,
            shift_hz: map::echo_spiral_hz(s.tone),
            diffusion: map::echo_diffusion(s.tone),
        };
        self.settings = *s;
    }

    /// Fully faded out (and not about to fade in).
    fn off(&self) -> bool {
        !self.settings.on && self.active < 1e-4
    }

    fn clear(&mut self) {
        self.lines.iter_mut().for_each(DelayLine::clear);
        self.sides.iter_mut().for_each(Side::clear);
        self.idle = true;
    }

    #[inline]
    fn feedback_path(side: &mut Side, c: &Coeffs, y: f32, sample_rate: f32) -> f32 {
        let v = match c.mode {
            ECHO_BAND => side.svf.process(y, &c.band).1 * c.band_gain,
            ECHO_SPIRAL => {
                let s = side.shifter.process(y, c.shift_hz, sample_rate);
                side.lp += (s - side.lp) * c.lp;
                side.lp
            }
            ECHO_DIFFUSE => {
                let mut s = y;
                for d in side.diffusers.iter_mut() {
                    s = d.process(s, c.diffusion);
                }
                side.lp += (s - side.lp) * c.lp;
                side.lp
            }
            _ => {
                side.lp += (y - side.lp) * c.lp;
                side.lp
            }
        };
        side.hp += (v - side.hp) * c.hp;
        fast_tanh(v - side.hp)
    }

    /// Process a block in place, adding the echoes to `left`/`right`.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.off() {
            if !self.idle {
                self.clear();
            }
            return;
        }
        let s = self.settings;
        let sr = self.sample_rate;
        let target = [s.left_seconds, s.left_seconds * s.ratio];
        let on = if s.on { 1.0 } else { 0.0 };
        let tape = s.mode == ECHO_TAPE;
        let c = self.coeffs;
        let mut energy = 0.0f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.active += (on - self.active) * self.k_active;
            for (t, goal) in self.time.iter_mut().zip(target) {
                *t += (goal - *t) * self.k_time;
            }
            let speed = if tape {
                self.wow = (self.wow + WOW_HZ / sr).fract();
                self.flutter = (self.flutter + FLUTTER_HZ / sr).fract();
                1.0 + WOW_DEPTH * sin_turns(self.wow) + FLUTTER_DEPTH * sin_turns(self.flutter)
            } else {
                1.0
            };
            let yl = self.lines[0].read(self.time[0] * speed * sr);
            // The right head wobbles against the left: a slightly different
            // point on the tape loop.
            let speed_r = if tape {
                1.0 + WOW_DEPTH * sin_turns(self.wow + 0.3)
                    + FLUTTER_DEPTH * sin_turns(self.flutter + 0.5)
            } else {
                1.0
            };
            let yr = self.lines[1].read(self.time[1] * speed_r * sr);
            let input = 0.5 * (*l + *r) * self.active;
            let fl = Self::feedback_path(&mut self.sides[0], &c, yr, sr);
            let fr = Self::feedback_path(&mut self.sides[1], &c, yl, sr);
            self.lines[0].write(input + s.feedback * fl);
            self.lines[1].write(s.feedback * fr);
            let wet = s.level * self.active;
            *l += wet * yl;
            *r += wet * yr;
            energy += yl * yl + yr * yr + input * input;
        }
        self.idle = energy < 1e-12;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn settings(mode: u32, feedback: f32) -> EchoSettings {
        EchoSettings {
            on: true,
            left_seconds: 0.1,
            ratio: 1.0,
            mode,
            tone: 0.8,
            feedback,
            level: 1.0,
        }
    }

    fn impulse_response(e: &mut Echo, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        // Let the on-fade settle, then an impulse.
        // Long enough for the tape-motor glide to reach the set time.
        let mut warm_l = vec![0.0; 96_000];
        let mut warm_r = vec![0.0; 96_000];
        e.process(&mut warm_l, &mut warm_r);
        l[0] = 1.0;
        r[0] = 1.0;
        for (a, b) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            e.process(a, b);
        }
        (l, r)
    }

    #[test]
    fn hilbert_outputs_are_in_quadrature() {
        for hz in [80.0, 440.0, 3000.0, 12_000.0] {
            let mut h = Hilbert::default();
            let mut mags = vec![];
            for i in 0..48_000 {
                let x = (core::f32::consts::TAU * hz * i as f32 / SR).sin();
                let (a, b) = h.process(x);
                if i > 24_000 {
                    mags.push((a * a + b * b).sqrt());
                }
            }
            let (lo, hi) = mags
                .iter()
                .fold((f32::MAX, 0.0f32), |(lo, hi), &m| (lo.min(m), hi.max(m)));
            assert!(hi - lo < 0.03, "{hz} Hz: {lo}..{hi}");
        }
    }

    #[test]
    fn frequency_shifter_shifts_up() {
        let mut fs = FreqShifter::default();
        let out: Vec<f32> = (0..96_000)
            .map(|i| {
                fs.process(
                    (core::f32::consts::TAU * 1000.0 * i as f32 / SR).sin(),
                    100.0,
                    SR,
                )
            })
            .collect();
        let tail = &out[48_000..];
        let crossings = tail
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let hz = crossings as f32 / (tail.len() as f32 / SR);
        assert!((hz - 1100.0).abs() < 5.0, "{hz}");
    }

    #[test]
    fn ping_pong_alternates() {
        let mut e = Echo::new(SR);
        e.prepare(&settings(ECHO_TAPE, 0.5));
        let (l, r) = impulse_response(&mut e, 24_000);
        let peak = |x: &[f32], at: f32| {
            let c = (at * SR) as usize;
            x[c - 400..c + 400]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
        };
        // First repeat left, second right, third left.
        assert!(peak(&l, 0.1) > 0.3 && peak(&r, 0.1) < 0.01);
        assert!(peak(&r, 0.2) > 0.1 && peak(&l, 0.2) < 0.01);
        assert!(peak(&l, 0.3) > 0.03);
    }

    #[test]
    fn all_modes_are_stable_at_max_feedback() {
        for mode in [ECHO_TAPE, ECHO_BAND, ECHO_SPIRAL, ECHO_DIFFUSE] {
            for tone in [0.0, 0.5, 1.0] {
                let mut e = Echo::new(SR);
                let mut s = settings(mode, 1.1);
                s.tone = tone;
                s.ratio = 0.618;
                s.left_seconds = 0.03;
                e.prepare(&s);
                let (l, r) = impulse_response(&mut e, 48_000 * 5);
                assert!(
                    l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 2.2),
                    "mode {mode} tone {tone}"
                );
            }
        }
    }

    #[test]
    fn off_is_silent_and_clean() {
        let mut e = Echo::new(SR);
        let mut s = settings(ECHO_TAPE, 0.9);
        e.prepare(&s);
        let (l, _) = impulse_response(&mut e, 9_600);
        assert!(l.iter().skip(1).any(|x| x.abs() > 0.1));
        s.on = false;
        e.prepare(&s);
        let mut a = vec![0.0; 48_000];
        let mut b = vec![0.0; 48_000];
        e.process(&mut a, &mut b);
        let mut a = vec![0.5; 9600];
        let mut b = vec![0.5; 9600];
        e.process(&mut a, &mut b);
        assert!(a.iter().chain(&b).all(|&x| x == 0.5));
    }
}
