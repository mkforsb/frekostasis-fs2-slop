//! Two-channel engine: runs both voices at 2× the output rate, cross-coupled
//! for XMOD and RING (each channel hears the other's previous sample),
//! decimates each one, sends it through its own plate reverb, pans, mixes and
//! applies the output stage.

use crate::params::{Param, VoicePatch, map};
use crate::reverb::Plate;
use crate::voice::{Voice, fast_tanh};

pub const VOICES: usize = 2;
pub const OVERSAMPLE: usize = 2;
pub const DEFAULT_MASTER: f32 = 0.8;

/// Windowed-sinc lowpass + 2:1 decimator.
#[derive(Clone, Debug)]
struct Decimator {
    taps: [f32; Self::TAPS],
    /// Doubled history so the dot product is always over a contiguous slice.
    hist: [f32; 2 * Self::TAPS],
    pos: usize,
}

impl Decimator {
    const TAPS: usize = 47;

    fn new() -> Self {
        // Cutoff at 0.23 × the oversampled rate (≈ 22 kHz at 96 kHz).
        let fc = 0.23f32;
        let m = (Self::TAPS - 1) as f32 / 2.0;
        let mut taps = [0.0f32; Self::TAPS];
        for (n, t) in taps.iter_mut().enumerate() {
            let x = n as f32 - m;
            let sinc = if x == 0.0 {
                2.0 * fc
            } else {
                (2.0 * core::f32::consts::PI * fc * x).sin() / (core::f32::consts::PI * x)
            };
            let w = n as f32 / (Self::TAPS - 1) as f32 * core::f32::consts::TAU;
            let blackman = 0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos();
            *t = sinc * blackman;
        }
        let sum: f32 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum);
        Self {
            taps,
            hist: [0.0; 2 * Self::TAPS],
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = if self.pos == 0 {
            Self::TAPS - 1
        } else {
            self.pos - 1
        };
        self.hist[self.pos] = x;
        self.hist[self.pos + Self::TAPS] = x;
    }

    #[inline]
    fn process(&mut self, block: [f32; OVERSAMPLE]) -> f32 {
        for x in block {
            self.push(x);
        }
        let h = &self.hist[self.pos..self.pos + Self::TAPS];
        h.iter().zip(self.taps.iter()).map(|(a, b)| a * b).sum()
    }
}

#[derive(Clone, Debug, Default)]
struct DcBlocker {
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    #[inline]
    fn process(&mut self, x: f32, r: f32) -> f32 {
        let y = x - self.x1 + r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    sample_rate: f32,
    voices: [Voice; VOICES],
    master_target: f32,
    master: f32,
    k_master: f32,
    dc_r: f32,
    /// One decimator per voice (voices are mono until panned).
    dec: [Decimator; VOICES],
    reverbs: [Plate; VOICES],
    dc: [DcBlocker; 2],
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate * OVERSAMPLE as f32;
        Self {
            sample_rate,
            voices: [Voice::new(fs, 1), Voice::new(fs, 2)],
            master_target: DEFAULT_MASTER,
            master: DEFAULT_MASTER,
            k_master: 1.0 - (-1.0 / (0.01 * sample_rate)).exp(),
            dc_r: 1.0 - core::f32::consts::TAU * 8.0 / sample_rate,
            dec: [Decimator::new(), Decimator::new()],
            reverbs: [Plate::new(sample_rate), Plate::new(sample_rate)],
            dc: [DcBlocker::default(), DcBlocker::default()],
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_param(&mut self, voice: usize, param: Param, value: f32) {
        if let Some(v) = self.voices.get_mut(voice) {
            v.set_param(param, value);
        }
    }

    pub fn param(&self, voice: usize, param: Param) -> f32 {
        self.voices[voice].param(param)
    }

    pub fn load_patch(&mut self, voice: usize, patch: &VoicePatch) {
        if let Some(v) = self.voices.get_mut(voice) {
            for p in crate::params::ALL_PARAMS {
                v.set_param(p, patch[p.index()]);
            }
        }
    }

    /// Jump smoothed parameters straight to their targets.
    pub fn snap_params(&mut self) {
        self.voices.iter_mut().for_each(Voice::snap_params);
        self.master = self.master_target;
    }

    /// Master volume, `0..=1` knob position.
    pub fn set_master(&mut self, value: f32) {
        self.master_target = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            DEFAULT_MASTER
        };
    }

    /// Jump one voice's smoothed parameters to their targets, e.g. so a patch
    /// change sent together with a trigger takes effect exactly on the hit.
    pub fn snap_voice(&mut self, voice: usize) {
        if let Some(v) = self.voices.get_mut(voice) {
            v.snap_params();
        }
    }

    /// Bit `v` set for every voice a hit on `voice` fires: the voice itself
    /// plus any other voice with LINK on. Linking is one level deep, so two
    /// channels linked to each other simply fire together.
    pub fn fired_by(&self, voice: usize) -> u32 {
        if voice >= VOICES {
            return 0;
        }
        (0..VOICES).fold(1 << voice, |acc, o| {
            if o != voice && self.voices[o].linked() {
                acc | (1 << o)
            } else {
                acc
            }
        })
    }

    /// Hit `voice` (and the voices linked to it).
    pub fn trigger(&mut self, voice: usize, velocity: f32) {
        self.trigger_snapped(voice, velocity, false);
    }

    /// Like [`Engine::trigger`]; with `snap` every voice that fires first
    /// jumps to its pending parameter values, so a new patch lands exactly
    /// on the hit.
    pub fn trigger_snapped(&mut self, voice: usize, velocity: f32, snap: bool) {
        let fired = self.fired_by(voice);
        for (i, v) in self.voices.iter_mut().enumerate() {
            if fired & (1 << i) != 0 {
                if snap {
                    v.snap_params();
                }
                v.trigger(velocity);
            }
        }
    }

    pub fn envelope(&self, voice: usize) -> f32 {
        self.voices[voice].envelope()
    }

    /// Render stereo audio. `left` and `right` must be the same length.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let pans: [(f32, f32); VOICES] = core::array::from_fn(|i| {
            // Equal-power pan, normalized so the centre is unity per side.
            let a = self.voices[i].param(Param::Pan) * core::f32::consts::FRAC_PI_2;
            (
                a.cos() * core::f32::consts::SQRT_2,
                a.sin() * core::f32::consts::SQRT_2,
            )
        });
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mut blocks = [[0.0; OVERSAMPLE]; VOICES];
            let [a, b] = &mut self.voices;
            let [block_a, block_b] = &mut blocks;
            for (ya, yb) in block_a.iter_mut().zip(block_b.iter_mut()) {
                // Each channel hears the other's previous sample.
                let (xa, xb) = (a.xsig(), b.xsig());
                *ya = a.tick(xb);
                *yb = b.tick(xa);
            }
            let (mut yl, mut yr) = (0.0, 0.0);
            for (((v, (dec, block)), plate), (gl, gr)) in self
                .voices
                .iter()
                .zip(self.dec.iter_mut().zip(blocks))
                .zip(self.reverbs.iter_mut())
                .zip(pans)
            {
                let dry = dec.process(block);
                // Post-fader send into the channel's own plate. The plate always
                // runs so the tail keeps ringing when AMOUNT is turned down.
                let controls = plate.controls(
                    v.smoothed(Param::ReverbDecay),
                    v.smoothed(Param::ReverbTone),
                    v.smoothed(Param::ReverbPredelay),
                );
                let (wl, wr) = plate.process(dry, &controls);
                let send = map::reverb_gain(v.smoothed(Param::ReverbMix));
                // Pan acts as a balance control on the stereo wet signal.
                yl += (dry + send * wl) * gl;
                yr += (dry + send * wr) * gr;
            }
            self.master += (self.master_target - self.master) * self.k_master;
            let gain = map::level_gain(self.master) * 1.4;
            *l = soft_clip(self.dc[0].process(yl, self.dc_r) * gain);
            *r = soft_clip(self.dc[1].process(yr, self.dc_r) * gain);
        }
    }

    /// Convenience for offline rendering.
    pub fn render_vec(&mut self, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        self.render(&mut l, &mut r);
        (l, r)
    }
}

/// Output stage: transparent below ~-6 dBFS, gently saturating above.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.5;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        (KNEE + (1.0 - KNEE) * fast_tanh(over)).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{ALGORITHMS, ALL_PARAMS, OpParam, ParamKind, ratio_knob};

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Rising zero crossings per second.
    fn zc_hz(x: &[f32], sr: f32) -> f32 {
        let n = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        n as f32 / (x.len() as f32 / sr)
    }

    /// A single sine: only operator 1 sounds, no sweep, long decays.
    fn pure_sine(e: &mut Engine, v: usize, tune: f32) {
        e.set_param(v, Param::Algorithm, 7.0);
        e.set_param(v, Param::Tune, tune);
        e.set_param(v, Param::Decay, 1.0);
        e.set_param(v, Param::Op1Decay, 1.0);
        e.set_param(v, Param::Op1Level, 0.8);
        for op in 1..4 {
            e.set_param(v, OpParam::Level.of(op), 0.0);
        }
    }

    #[test]
    fn silent_until_triggered() {
        let mut e = Engine::new(48_000.0);
        let (l, r) = e.render_vec(4_800);
        assert!(rms(&l) < 1e-4 && rms(&r) < 1e-4);
    }

    #[test]
    fn every_algorithm_makes_sound_and_decays() {
        for (n, _) in ALGORITHMS.iter().enumerate() {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::Algorithm, n as f32);
            e.set_param(0, Param::Decay, 0.3);
            for op in 0..4 {
                e.set_param(0, OpParam::Level.of(op), 0.6);
            }
            e.set_param(0, Param::Feedback, 0.4);
            e.snap_params();
            e.trigger(0, 1.0);
            let (l, _) = e.render_vec(4_800);
            let early = rms(&l);
            assert!(early > 0.01, "algorithm {n} too quiet: {early}");
            let (l, _) = e.render_vec(48_000 * 4);
            let late = rms(&l[l.len() - 4_800..]);
            assert!(late < early * 0.001, "algorithm {n} did not decay: {late}");
        }
    }

    #[test]
    fn output_is_finite_and_bounded_under_random_patches() {
        let mut rng = crate::voice::Noise::new(99);
        let mut e = Engine::new(44_100.0);
        for round in 0..80 {
            for v in 0..VOICES {
                for p in ALL_PARAMS {
                    let x = rng.sample() * 0.5 + 0.5;
                    let val = match p.info().kind {
                        ParamKind::Continuous => x,
                        ParamKind::Choice(n) => (x * n.len() as f32).floor(),
                    };
                    e.set_param(v, p, val);
                }
                e.trigger(v, rng.sample() * 0.5 + 0.5);
            }
            e.set_master(1.0);
            let (l, r) = e.render_vec(2_205);
            for s in l.iter().chain(r.iter()) {
                assert!(s.is_finite() && s.abs() <= 1.0, "round {round}: {s}");
            }
        }
    }

    #[test]
    fn pitch_tracks_tune_and_ratio() {
        let sr = 48_000.0;
        for (tune, ratio) in [(0.3f32, 1.0f32), (0.5, 1.0), (0.4, 2.0), (0.4, 0.5)] {
            let mut e = Engine::new(sr);
            pure_sine(&mut e, 0, tune);
            e.set_param(0, Param::Op1Ratio, ratio_knob(ratio));
            e.snap_params();
            e.trigger(0, 1.0);
            e.render_vec(2_400);
            let (l, _) = e.render_vec(24_000);
            let measured = zc_hz(&l, sr);
            let expected = map::tune_hz(tune, 0.5) * ratio;
            let cents = 1200.0 * (measured / expected).log2();
            assert!(cents.abs() < 10.0, "{measured} Hz vs {expected} Hz");
        }
    }

    #[test]
    fn modulation_adds_harmonics() {
        // With a modulator the zero-crossing rate climbs well above the
        // carrier's own pitch.
        let sr = 48_000.0;
        let brightness = |level: f32| {
            let mut e = Engine::new(sr);
            pure_sine(&mut e, 0, 0.35);
            e.set_param(0, Param::Algorithm, 0.0);
            e.set_param(0, Param::Op2Level, level);
            e.set_param(0, Param::Op2Decay, 1.0);
            e.set_param(0, Param::Op2Ratio, ratio_knob(3.0));
            e.snap_params();
            e.trigger(0, 1.0);
            let (l, _) = e.render_vec(9_600);
            let d: Vec<f32> = l.windows(2).map(|w| w[1] - w[0]).collect();
            rms(&d) / rms(&l)
        };
        assert!(brightness(0.7) > brightness(0.0) * 2.0);
    }

    #[test]
    fn sweep_down_starts_high() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        pure_sine(&mut e, 0, 0.3);
        e.set_param(0, Param::SweepAmount, 0.85);
        e.set_param(0, Param::SweepTime, 0.5);
        e.snap_params();
        e.trigger(0, 1.0);
        let (l, _) = e.render_vec(24_000);
        let first = zc_hz(&l[..2_400], sr);
        let last = zc_hz(&l[l.len() - 4_800..], sr);
        assert!(first > last * 2.0, "sweep down: {first} vs {last}");
    }

    #[test]
    fn higher_velocity_is_louder() {
        let level = |vel: f32| {
            let mut e = Engine::new(48_000.0);
            e.snap_params();
            e.trigger(0, vel);
            rms(&e.render_vec(4_800).0)
        };
        assert!(level(1.0) > level(0.3) * 2.0);
    }

    #[test]
    fn noise_oscillator_sounds_alone() {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Op1Level, 0.0);
        e.set_param(0, Param::Op2Level, 0.0);
        e.set_param(0, Param::NoiseLevel, 0.8);
        e.snap_params();
        e.trigger(0, 1.0);
        let (l, _) = e.render_vec(4_800);
        assert!(rms(&l) > 0.01);
    }

    fn hit(configure: impl Fn(&mut Engine), frames: usize) -> (Vec<f32>, Vec<f32>) {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Decay, 0.25);
        configure(&mut e);
        e.snap_params();
        e.trigger(0, 1.0);
        e.render_vec(frames)
    }

    #[test]
    fn reverb_at_zero_amount_leaves_the_dry_signal_untouched() {
        let dry = hit(|_| {}, 24_000);
        let other = hit(
            |e| {
                e.set_param(0, Param::ReverbDecay, 1.0);
                e.set_param(0, Param::ReverbTone, 0.0);
                e.set_param(0, Param::ReverbPredelay, 1.0);
            },
            24_000,
        );
        assert_eq!(dry, other);
    }

    #[test]
    fn reverb_adds_a_tail() {
        let frames = 48_000 * 2;
        let with_reverb = |amount: f32| {
            let (l, r) = hit(
                |e| {
                    e.set_param(0, Param::ReverbMix, amount);
                    e.set_param(0, Param::ReverbDecay, 0.6);
                },
                frames,
            );
            rms(&l[48_000..72_000]) + rms(&r[48_000..72_000])
        };
        let dry = with_reverb(0.0);
        let wet = with_reverb(0.7);
        assert!(dry < 1e-5, "dry tail {dry}");
        assert!(wet > 1e-3, "reverb tail {wet}");
    }

    #[test]
    fn pan_hard_left_with_reverb() {
        let (l, r) = hit(
            |e| {
                e.set_param(0, Param::Pan, 0.0);
                e.set_param(0, Param::ReverbMix, 0.8);
            },
            24_000,
        );
        assert!(rms(&r) < rms(&l) * 0.01);
    }

    /// Channel 1 a pure sine; channel 2 a sine at an unrelated pitch, panned
    /// hard right so the left output is channel 1 alone.
    fn cross_engine(configure: impl Fn(&mut Engine)) -> Engine {
        let mut e = Engine::new(48_000.0);
        pure_sine(&mut e, 0, 0.45);
        pure_sine(&mut e, 1, 0.61);
        e.set_param(0, Param::Pan, 0.0);
        e.set_param(1, Param::Pan, 1.0);
        configure(&mut e);
        e.snap_params();
        e
    }

    fn left_after(e: &mut Engine, trigger_both: bool) -> Vec<f32> {
        e.trigger(0, 1.0);
        if trigger_both {
            e.trigger(1, 1.0);
        }
        e.render_vec(9_600).0
    }

    #[test]
    fn xmod_needs_the_other_channel_to_sound() {
        let plain = left_after(&mut cross_engine(|_| {}), true);
        let alone = left_after(
            &mut cross_engine(|e| e.set_param(0, Param::XmodAmount, 0.8)),
            false,
        );
        let crossed = left_after(
            &mut cross_engine(|e| e.set_param(0, Param::XmodAmount, 0.8)),
            true,
        );
        let diff = |a: &[f32], b: &[f32]| {
            let d: Vec<f32> = a.iter().zip(b).map(|(x, y)| x - y).collect();
            rms(&d)
        };
        assert!(diff(&plain, &alone) < 1e-6, "XMOD with a silent partner");
        assert!(
            diff(&plain, &crossed) > 0.05 * rms(&plain),
            "XMOD had no effect"
        );
    }

    #[test]
    fn ring_needs_the_other_channel_to_sound() {
        let alone = left_after(
            &mut cross_engine(|e| e.set_param(0, Param::Ring, 1.0)),
            false,
        );
        let both = left_after(
            &mut cross_engine(|e| e.set_param(0, Param::Ring, 1.0)),
            true,
        );
        assert!(
            rms(&alone) < 1e-4,
            "full ring with a silent partner: {}",
            rms(&alone)
        );
        assert!(rms(&both) > 0.01);
    }

    #[test]
    fn link_fires_the_other_channel() {
        let mut e = cross_engine(|e| e.set_param(1, Param::Link, 1.0));
        assert_eq!(e.fired_by(0), 0b11);
        assert_eq!(e.fired_by(1), 0b10);
        e.trigger(0, 1.0);
        let (_, r) = e.render_vec(4_800);
        assert!(rms(&r) > 0.01, "linked channel stayed silent");
    }

    #[test]
    fn snap_voice_applies_pending_changes_to_that_voice_only() {
        let mut e = Engine::new(48_000.0);
        e.snap_params();
        for v in 0..VOICES {
            e.set_param(v, Param::Tune, 0.9);
        }
        e.snap_voice(0);
        assert_eq!(e.voices[0].smoothed(Param::Tune), 0.9);
        assert_ne!(
            e.voices[1].smoothed(Param::Tune),
            0.9,
            "voice 2 should still glide"
        );
    }

    #[test]
    fn retrigger_does_not_click() {
        // Hitting a ringing voice again resets its phases; the declick keeps
        // the jump small.
        let mut e = Engine::new(48_000.0);
        pure_sine(&mut e, 0, 0.3);
        e.snap_params();
        e.trigger(0, 1.0);
        let (a, _) = e.render_vec(1_234);
        e.trigger(0, 1.0);
        let (b, _) = e.render_vec(64);
        let max_step = a
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        let jump = (b[0] - a[a.len() - 1]).abs();
        assert!(
            jump < max_step * 2.0,
            "jump {jump} vs normal step {max_step}"
        );
    }
}
