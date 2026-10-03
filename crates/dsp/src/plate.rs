//! Shimmer plate: the Dattorro plate of the FK-2 (and XK-1) (J. Dattorro, "Effect Design
//! Part 1: Reverberator and Other Filters", JAES 45(9), 1997), enlarged, with
//! a pitch shifter inside each half of the tank.
//!
//! The classic shimmer feeds a pitch-shifted copy of the reverb's *output*
//! back into its *input*. That loop's gain is the reverb's gain for a steady
//! signal, which for a 60-second tail is far above one, so it needs limiting
//! to survive. Here the shifters sit in the tank itself, between the first
//! delay and the damping filter: on every trip round the figure-of-eight a
//! SHIMMER fraction of the signal is replaced by its shifted copy. The
//! shifter never adds energy (its two grains are crossfaded with
//! `sin² + cos² = 1`), so the loop gain stays the tank's decay gain, below
//! one, however high DECAY and SHIMMER are. Each pass climbs another
//! interval and the damping filter eats the top, which gives the endless
//! rising halo.

use crate::util::{one_pole_coeff, sin_turns};

use crate::params::{Param, Patch, SHIMMER_SEMITONES, map};

const REF_SR: f32 = 29_761.0;
/// Every delay is this much longer than Dattorro's: a bigger, darker room.
const SIZE: f32 = 1.4;

const INPUT_DIFFUSERS: [(usize, f32); 4] = [(142, 0.75), (107, 0.75), (379, 0.625), (277, 0.625)];
const L_MOD_AP: usize = 672;
const L_DELAY_1: usize = 4453;
const L_AP: usize = 1800;
const L_DELAY_2: usize = 3720;
const R_MOD_AP: usize = 908;
const R_DELAY_1: usize = 4217;
const R_AP: usize = 2656;
const R_DELAY_2: usize = 3163;
const EXCURSION: f32 = 16.0;
const MOD_HZ: f32 = 0.5;
const DECAY_DIFFUSION_1: f32 = 0.7;
const LOOP_SECONDS: f32 = SIZE
    * (L_MOD_AP + L_DELAY_1 + L_AP + L_DELAY_2 + R_MOD_AP + R_DELAY_1 + R_AP + R_DELAY_2) as f32
    / REF_SR;
pub const MAX_PREDELAY_S: f32 = 0.4;
/// Pitch-shifter grain length.
const GRAIN_SECONDS: f32 = 0.09;
const ON_OFF_SECONDS: f32 = 0.05;
const PARAM_SMOOTH_SECONDS: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
enum Src {
    LDelay1,
    LAp,
    LDelay2,
    RDelay1,
    RAp,
    RDelay2,
}

const LEFT_TAPS: [(Src, usize, f32); 7] = [
    (Src::RDelay1, 266, 1.0),
    (Src::RDelay1, 2974, 1.0),
    (Src::RAp, 1913, -1.0),
    (Src::RDelay2, 1996, 1.0),
    (Src::LDelay1, 1990, -1.0),
    (Src::LAp, 187, -1.0),
    (Src::LDelay2, 1066, -1.0),
];

const RIGHT_TAPS: [(Src, usize, f32); 7] = [
    (Src::LDelay1, 353, 1.0),
    (Src::LDelay1, 3627, 1.0),
    (Src::LAp, 1228, -1.0),
    (Src::LDelay2, 2673, 1.0),
    (Src::RDelay1, 2111, -1.0),
    (Src::RAp, 335, -1.0),
    (Src::RDelay2, 121, -1.0),
];

#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-18 { 0.0 } else { x }
}

/// Power-of-two ring buffer.
#[derive(Clone, Debug)]
struct Delay {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
    len: usize,
}

impl Delay {
    fn new(len: usize, headroom: usize) -> Self {
        let size = (len + headroom + 2).next_power_of_two();
        Self {
            buf: vec![0.0; size],
            mask: size - 1,
            pos: 0,
            len: len.max(1),
        }
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }

    #[inline]
    fn tap(&self, d: usize) -> f32 {
        self.buf[self.pos.wrapping_sub(d) & self.mask]
    }

    #[inline]
    fn tap_frac(&self, d: f32) -> f32 {
        let d = d.max(0.0);
        let i = d as usize;
        let f = d - i as f32;
        let a = self.tap(i);
        a + (self.tap(i + 1) - a) * f
    }

    #[inline]
    fn output(&self) -> f32 {
        self.tap(self.len - 1)
    }

    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

#[derive(Clone, Debug)]
struct Allpass {
    d: Delay,
}

impl Allpass {
    fn new(len: usize, headroom: usize) -> Self {
        Self {
            d: Delay::new(len, headroom),
        }
    }

    #[inline]
    fn process(&mut self, x: f32, g: f32) -> f32 {
        let delayed = self.d.output();
        let v = x - g * delayed;
        self.d.write(flush(v));
        delayed + g * v
    }

    #[inline]
    fn process_mod(&mut self, x: f32, g: f32, offset: f32) -> f32 {
        let delayed = self.d.tap_frac(self.d.len as f32 - 1.0 + offset);
        let v = x - g * delayed;
        self.d.write(flush(v));
        delayed + g * v
    }
}

/// Delay-line pitch shifter: two read heads sweep through a grain-long
/// window at the speed that gives the ratio, crossfaded so that one is
/// silent whenever the other jumps.
#[derive(Clone, Debug)]
pub struct Shifter {
    d: Delay,
    phase: f32,
    grain: f32,
}

impl Shifter {
    pub fn new(sample_rate: f32) -> Self {
        let grain = GRAIN_SECONDS * sample_rate;
        Self {
            d: Delay::new(grain as usize + 4, 0),
            phase: 0.0,
            grain,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32, ratio: f32) -> f32 {
        self.d.write(x);
        self.phase += (1.0 - ratio) / self.grain;
        self.phase -= self.phase.floor();
        let p2 = (self.phase + 0.5).fract();
        let w1 = sin_turns(0.5 * self.phase);
        let w2 = sin_turns(0.5 * p2);
        self.d.tap_frac(self.phase * self.grain + 1.0) * w1 * w1
            + self.d.tap_frac(p2 * self.grain + 1.0) * w2 * w2
    }

    fn clear(&mut self) {
        self.d.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerbSettings {
    pub on: bool,
    pub mix: f32,
    pub decay_seconds: f32,
    pub tone_hz: f32,
    pub predelay_seconds: f32,
    /// Fraction of each tank pass replaced by its shifted copy.
    pub shimmer: f32,
    /// Shift of the left and right tank halves, semitones.
    pub shift: (f32, f32),
}

impl VerbSettings {
    pub fn from_patch(p: &Patch) -> Self {
        let v = |param: Param| param.sanitize(p[param.index()]);
        Self {
            on: v(Param::VerbOn) > 0.5,
            mix: v(Param::VerbMix),
            decay_seconds: map::verb_seconds(v(Param::VerbDecay)),
            tone_hz: map::verb_tone_hz(v(Param::VerbTone)),
            predelay_seconds: map::verb_predelay_seconds(v(Param::VerbPre)),
            shimmer: map::shimmer_mix(v(Param::Shimmer)),
            shift: SHIMMER_SEMITONES[v(Param::ShimmerInterval) as usize],
        }
    }
}

/// Smoothed per-sample controls.
#[derive(Clone, Copy, Debug)]
struct Controls {
    decay: f32,
    tone: f32,
    predelay: f32,
    shimmer: f32,
    mix: f32,
}

pub struct ShimmerPlate {
    sample_rate: f32,
    predelay: Delay,
    bandwidth: f32,
    input: [Allpass; 4],
    input_g: [f32; 4],
    l_mod_ap: Allpass,
    l_delay_1: Delay,
    l_damp: f32,
    l_ap: Allpass,
    l_delay_2: Delay,
    r_mod_ap: Allpass,
    r_delay_1: Delay,
    r_damp: f32,
    r_ap: Allpass,
    r_delay_2: Delay,
    shifters: [Shifter; 2],
    excursion: f32,
    lfo: f32,
    lfo_inc: f32,
    left_taps: [(Src, usize, f32); 7],
    right_taps: [(Src, usize, f32); 7],
    settings: VerbSettings,
    ctl: Controls,
    k_smooth: f32,
    active: f32,
    k_active: f32,
    idle: bool,
}

impl ShimmerPlate {
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / REF_SR * SIZE;
        let n = |len: usize| ((len as f32 * scale).round() as usize).max(1);
        let excursion = EXCURSION * sample_rate / REF_SR;
        let exc_room = excursion.ceil() as usize + 2;
        let scale_taps =
            |taps: [(Src, usize, f32); 7]| taps.map(|(s, off, sign)| (s, n(off), sign));
        let settings = VerbSettings::from_patch(&crate::params::default_patch());
        let mut plate = Self {
            sample_rate,
            predelay: Delay::new((MAX_PREDELAY_S * sample_rate) as usize + 2, 2),
            bandwidth: 0.0,
            input: INPUT_DIFFUSERS.map(|(len, _)| Allpass::new(n(len), 0)),
            input_g: INPUT_DIFFUSERS.map(|(_, g)| g),
            l_mod_ap: Allpass::new(n(L_MOD_AP), exc_room),
            l_delay_1: Delay::new(n(L_DELAY_1), 0),
            l_damp: 0.0,
            l_ap: Allpass::new(n(L_AP), 0),
            l_delay_2: Delay::new(n(L_DELAY_2), 0),
            r_mod_ap: Allpass::new(n(R_MOD_AP), exc_room),
            r_delay_1: Delay::new(n(R_DELAY_1), 0),
            r_damp: 0.0,
            r_ap: Allpass::new(n(R_AP), 0),
            r_delay_2: Delay::new(n(R_DELAY_2), 0),
            shifters: [Shifter::new(sample_rate), Shifter::new(sample_rate)],
            excursion,
            lfo: 0.0,
            lfo_inc: MOD_HZ / sample_rate,
            left_taps: scale_taps(LEFT_TAPS),
            right_taps: scale_taps(RIGHT_TAPS),
            settings,
            ctl: Controls {
                decay: 0.0,
                tone: 0.0,
                predelay: 0.0,
                shimmer: 0.0,
                mix: 0.0,
            },
            k_smooth: one_pole_coeff(PARAM_SMOOTH_SECONDS, sample_rate),
            active: 0.0,
            k_active: one_pole_coeff(ON_OFF_SECONDS, sample_rate),
            idle: true,
        };
        plate.ctl = plate.targets(&settings);
        plate
    }

    /// Tank gain that gives roughly `rt60` seconds of decay.
    pub fn decay_gain(rt60: f32) -> f32 {
        (-6.907_755 * LOOP_SECONDS / (4.0 * rt60.max(0.05)))
            .exp()
            .min(0.985)
    }

    fn targets(&self, s: &VerbSettings) -> Controls {
        let sr = self.sample_rate;
        Controls {
            decay: Self::decay_gain(s.decay_seconds),
            tone: 1.0 - (-core::f32::consts::TAU * s.tone_hz.min(0.45 * sr) / sr).exp(),
            predelay: (s.predelay_seconds * sr).min(MAX_PREDELAY_S * sr),
            shimmer: s.shimmer,
            mix: s.mix,
        }
    }

    pub fn prepare(&mut self, s: &VerbSettings) {
        self.settings = *s;
    }

    fn clear(&mut self) {
        self.predelay.clear();
        self.bandwidth = 0.0;
        for ap in self.input.iter_mut() {
            ap.d.clear();
        }
        for d in [
            &mut self.l_delay_1,
            &mut self.l_delay_2,
            &mut self.r_delay_1,
            &mut self.r_delay_2,
            &mut self.l_mod_ap.d,
            &mut self.l_ap.d,
            &mut self.r_mod_ap.d,
            &mut self.r_ap.d,
        ] {
            d.clear();
        }
        self.l_damp = 0.0;
        self.r_damp = 0.0;
        self.shifters.iter_mut().for_each(Shifter::clear);
        self.idle = true;
    }

    #[inline]
    fn read(&self, src: Src, offset: usize) -> f32 {
        match src {
            Src::LDelay1 => self.l_delay_1.tap(offset),
            Src::LAp => self.l_ap.d.tap(offset),
            Src::LDelay2 => self.l_delay_2.tap(offset),
            Src::RDelay1 => self.r_delay_1.tap(offset),
            Src::RAp => self.r_ap.d.tap(offset),
            Src::RDelay2 => self.r_delay_2.tap(offset),
        }
    }

    /// One mono sample in, the wet stereo pair out.
    #[inline]
    fn tick(&mut self, x: f32, c: &Controls, ratios: (f32, f32)) -> (f32, f32) {
        self.predelay.write(x);
        let x = self.predelay.tap_frac(c.predelay);
        self.bandwidth = flush(self.bandwidth + c.tone * (x - self.bandwidth));
        let mut s = self.bandwidth;
        for (ap, &g) in self.input.iter_mut().zip(&self.input_g) {
            s = ap.process(s, g);
        }
        self.lfo += self.lfo_inc;
        if self.lfo >= 1.0 {
            self.lfo -= 1.0;
        }
        let (mod_l, mod_r) = (
            self.excursion * sin_turns(self.lfo),
            self.excursion * sin_turns(self.lfo + 0.25),
        );
        let decay_diffusion_2 = (c.decay + 0.15).clamp(0.25, 0.5);
        let from_right = self.r_delay_2.output();
        let from_left = self.l_delay_2.output();

        let a = self
            .l_mod_ap
            .process_mod(s + c.decay * from_right, -DECAY_DIFFUSION_1, mod_l);
        let d = self.l_delay_1.output();
        self.l_delay_1.write(a);
        let d = if c.shimmer > 0.0 {
            d + c.shimmer * (self.shifters[0].process(d, ratios.0) - d)
        } else {
            d
        };
        self.l_damp = flush(self.l_damp + c.tone * (d - self.l_damp));
        let b = self.l_ap.process(self.l_damp * c.decay, decay_diffusion_2);
        self.l_delay_2.write(b);

        let a = self
            .r_mod_ap
            .process_mod(s + c.decay * from_left, -DECAY_DIFFUSION_1, mod_r);
        let d = self.r_delay_1.output();
        self.r_delay_1.write(a);
        let d = if c.shimmer > 0.0 {
            d + c.shimmer * (self.shifters[1].process(d, ratios.1) - d)
        } else {
            d
        };
        self.r_damp = flush(self.r_damp + c.tone * (d - self.r_damp));
        let b = self.r_ap.process(self.r_damp * c.decay, decay_diffusion_2);
        self.r_delay_2.write(b);

        let sum = |taps: &[(Src, usize, f32); 7]| {
            taps.iter()
                .map(|&(src, off, sign)| sign * self.read(src, off))
                .sum::<f32>()
                * 0.6
        };
        (sum(&self.left_taps), sum(&self.right_taps))
    }

    /// Process a block in place: equal-power crossfade from dry to wet by MIX.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let s = self.settings;
        if !s.on && self.active < 1e-4 {
            if !self.idle {
                self.clear();
            }
            return;
        }
        let target = self.targets(&s);
        let on = if s.on { 1.0 } else { 0.0 };
        let ratios = ((s.shift.0 / 12.0).exp2(), (s.shift.1 / 12.0).exp2());
        let k = self.k_smooth;
        let mut energy = 0.0f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let c = &mut self.ctl;
            c.decay += (target.decay - c.decay) * k;
            c.tone += (target.tone - c.tone) * k;
            c.predelay += (target.predelay - c.predelay) * k;
            c.shimmer += (target.shimmer - c.shimmer) * k;
            c.mix += (target.mix - c.mix) * k;
            self.active += (on - self.active) * self.k_active;
            let c = self.ctl;
            let x = 0.5 * (*l + *r) * self.active;
            let (wl, wr) = self.tick(x, &c, ratios);
            // Equal-power crossfade, faded towards dry when switched off.
            let a = c.mix * self.active * core::f32::consts::FRAC_PI_2;
            let (dry, wet) = (a.cos(), a.sin());
            *l = *l * dry + wl * wet;
            *r = *r * dry + wr * wet;
            energy += wl * wl + wr * wr + x * x;
        }
        self.idle = energy < 1e-12;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn settings(decay: f32, shimmer: f32, shift: (f32, f32)) -> VerbSettings {
        VerbSettings {
            on: true,
            mix: 1.0,
            decay_seconds: decay,
            tone_hz: 12_000.0,
            predelay_seconds: 0.0,
            shimmer,
            shift,
        }
    }

    fn run(
        p: &mut ShimmerPlate,
        input: impl Fn(usize) -> f32,
        frames: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let mut l: Vec<f32> = (0..frames).map(&input).collect();
        let mut r = l.clone();
        for (a, b) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            p.process(a, b);
        }
        (l, r)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn shifter_shifts_an_octave() {
        let mut s = Shifter::new(SR);
        let out: Vec<f32> = (0..96_000)
            .map(|i| s.process((core::f32::consts::TAU * 300.0 * i as f32 / SR).sin(), 2.0))
            .collect();
        // Dominant frequency via a coarse DFT scan.
        let tail = &out[48_000..];
        let power = |hz: f32| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, x) in tail.iter().enumerate() {
                let a = core::f32::consts::TAU * hz * i as f32 / SR;
                re += x * a.cos();
                im += x * a.sin();
            }
            re * re + im * im
        };
        assert!(power(600.0) > 20.0 * power(300.0));
    }

    #[test]
    fn stable_at_maximum_decay_and_shimmer() {
        for shift in SHIMMER_SEMITONES {
            let mut p = ShimmerPlate::new(SR);
            p.prepare(&settings(60.0, 0.8, shift));
            // Ten seconds of loud noise, then twenty of silence.
            let mut rng = crate::util::Noise::new(1);
            let noise: Vec<f32> = (0..SR as usize * 10).map(|_| rng.sample() * 0.5).collect();
            let (l, r) = run(
                &mut p,
                |i| noise.get(i).copied().unwrap_or(0.0),
                SR as usize * 30,
            );
            let peak = l.iter().chain(&r).fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak.is_finite() && peak < 4.0, "{shift:?}: peak {peak}");
            // Still decaying at the end, not building up.
            let late = rms(&l[l.len() - 48_000..]);
            let early = rms(&l[SR as usize * 10..SR as usize * 11]);
            assert!(late < early, "{shift:?}: {late} vs {early}");
        }
    }

    #[test]
    fn decays_away() {
        let mut p = ShimmerPlate::new(SR);
        p.prepare(&settings(2.0, 0.5, (12.0, 12.0)));
        let (l, _) = run(&mut p, |i| if i == 0 { 1.0 } else { 0.0 }, SR as usize * 8);
        assert!(rms(&l[..48_000]) > 1e-3);
        assert!(rms(&l[l.len() - 48_000..]) < rms(&l[..48_000]) * 1e-3);
    }

    #[test]
    fn off_passes_dry() {
        let mut p = ShimmerPlate::new(SR);
        let mut s = settings(5.0, 0.5, (12.0, 12.0));
        s.on = false;
        p.prepare(&s);
        let (l, r) = run(&mut p, |i| (i as f32 * 0.01).sin(), 48_000);
        for (i, (a, b)) in l.iter().zip(&r).enumerate() {
            let x = (i as f32 * 0.01).sin();
            assert!((a - x).abs() < 1e-6 && (b - x).abs() < 1e-6);
        }
    }
}
