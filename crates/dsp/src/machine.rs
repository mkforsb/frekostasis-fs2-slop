//! The whole machine: clock → headless FK-2 → pool → stretcher → echo →
//! shimmer plate → output.
//!
//! ```text
//!  CLOCK ──strike──┐
//!                  ▼
//!  FK-2 (MUTATE) ─render─► POOL ─► STRETCHER (layers, PaulStretch) ─► ECHO ─► PLATE ─► MASTER
//!       ▲  a slice per block   ▲
//!       └──── planner: keep enough hits ready for the pattern
//! ```
//!
//! Everything runs on the audio thread in bounded slices per block: the
//! FK-2 renders at [`RENDER_SPEED`]× real time while the pool is short, and
//! the stretcher paces its FFTs over each hop.

use std::rc::Rc;

use crate::util::{DcBlocker, Noise, fast_tanh, one_pole_coeff};

use crate::clock::{Clock, Pattern, pool_target};
use crate::echo::{Echo, EchoSettings};
use crate::params::{Param, Patch, default_patch, map};
use crate::plate::{ShimmerPlate, VerbSettings};
use crate::source::{Hit, HitRenderer, OUTLINE, Pool, SourceSettings};
use crate::stretch::{MAX_LAYERS, StretchSettings, Stretcher};
use crate::telemetry as t;

/// Largest block processed at once; longer buffers are split.
pub const BLOCK: usize = 128;
/// FK-2 frames rendered per output frame while hits are needed.
pub const RENDER_SPEED: usize = 4;
/// … and when the pool has run dry.
pub const RENDER_SPEED_URGENT: usize = 6;
/// Fade of the stretched stream after STOP.
const RELEASE_SECONDS: f32 = 2.5;
const START_SECONDS: f32 = 0.05;
/// Output gain at MASTER 0 dB.
const OUTPUT_GAIN: f32 = 1.3;

pub struct Machine {
    sample_rate: f32,
    patch: Patch,
    dirty: bool,

    source: SourceSettings,
    generation: u32,
    renderer: HitRenderer,
    pool: Pool,
    pool_target: usize,

    pattern: Pattern,
    step_seconds: f32,
    chance: f32,
    clock: Clock,
    rng: Noise,

    stretch: StretchSettings,
    stretcher: Stretcher,
    echo_settings: EchoSettings,
    echo: Echo,
    verb_settings: VerbSettings,
    plate: ShimmerPlate,

    playing: bool,
    strike_now: bool,
    /// STRIKE pressed on the panel: works even while frozen.
    manual_strike: bool,
    play_gain: f32,
    k_start: f32,
    k_release: f32,
    master: f32,
    k_master: f32,
    dc: [DcBlocker; 2],
    dc_r: f32,

    last_hit: Option<Rc<Hit>>,
    /// Strikes that came before any hit was ready; they fire as soon as one is.
    owed: u32,
    strikes: u32,
    starved: u32,
    peak: [f32; 2],
    scratch: [[f32; BLOCK]; 2],
}

impl Machine {
    /// `seed` makes the machine's randomness (mutations, phases, chance)
    /// reproducible.
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let patch = default_patch();
        let mut m = Self {
            sample_rate,
            patch,
            dirty: true,
            source: SourceSettings::from_patch(&patch),
            generation: 0,
            renderer: HitRenderer::new(sample_rate, seed ^ 0x51ed_270b),
            pool: Pool::default(),
            pool_target: 2,
            pattern: Pattern::from_patch(&patch),
            step_seconds: 1.0,
            chance: 1.0,
            clock: Clock::default(),
            rng: Noise::new(seed ^ 0x0c0f_fee0),
            stretch: StretchSettings::from_patch(&patch),
            stretcher: Stretcher::new(sample_rate, seed ^ 0x7ea5_e1ed),
            echo_settings: EchoSettings::from_patch(&patch),
            echo: Echo::new(sample_rate),
            verb_settings: VerbSettings::from_patch(&patch),
            plate: ShimmerPlate::new(sample_rate),
            playing: false,
            strike_now: false,
            manual_strike: false,
            play_gain: 0.0,
            k_start: one_pole_coeff(START_SECONDS, sample_rate),
            k_release: one_pole_coeff(RELEASE_SECONDS / 5.0, sample_rate),
            master: map::master_gain(Param::Master.default_value()),
            k_master: one_pole_coeff(0.02, sample_rate),
            dc: [DcBlocker::default(), DcBlocker::default()],
            dc_r: 1.0 - core::f32::consts::TAU * 12.0 / sample_rate,
            last_hit: None,
            owed: 0,
            strikes: 0,
            starved: 0,
            peak: [0.0; 2],
            scratch: [[0.0; BLOCK]; 2],
        };
        m.refresh();
        m
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn param(&self, p: Param) -> f32 {
        self.patch[p.index()]
    }

    pub fn set_param(&mut self, p: Param, value: f32) {
        let value = p.sanitize(value);
        if self.patch[p.index()] == value {
            return;
        }
        self.patch[p.index()] = value;
        if p.shapes_source() {
            self.generation = self.generation.wrapping_add(1);
        }
        self.dirty = true;
    }

    pub fn load_patch(&mut self, patch: &Patch) {
        for p in crate::params::ALL_PARAMS {
            self.set_param(p, patch[p.index()]);
        }
    }

    /// Jump the master level to its target (after restoring a saved patch).
    pub fn snap(&mut self) {
        self.refresh();
        self.master = map::master_gain(self.patch[Param::Master.index()]);
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// PLAY strikes at once and starts the pattern; STOP lets the drone fade
    /// out while the echo and reverb ring on.
    pub fn set_playing(&mut self, on: bool) {
        if on && !self.playing {
            self.clock.reset();
            self.strike_now = true;
        }
        if !on {
            self.owed = 0;
        }
        self.playing = on;
    }

    /// Strike a hit right now, outside the pattern (ignored while stopped).
    pub fn strike_now(&mut self) {
        if self.playing {
            self.manual_strike = true;
        }
    }

    fn refresh(&mut self) {
        let p = &self.patch;
        self.source = SourceSettings::from_patch(p);
        self.pattern = Pattern::from_patch(p);
        self.step_seconds = map::step_seconds(p[Param::StepTime.index()]);
        self.chance = p[Param::Chance.index()];
        self.stretch = StretchSettings::from_patch(p);
        self.echo_settings = EchoSettings::from_patch(p);
        self.verb_settings = VerbSettings::from_patch(p);
        self.echo.prepare(&self.echo_settings);
        self.plate.prepare(&self.verb_settings);
        self.plan();
        self.dirty = false;
    }

    fn plan(&mut self) {
        let render_seconds = self.renderer.average_seconds() / RENDER_SPEED as f32;
        self.pool_target = pool_target(&self.pattern, self.step_seconds, render_seconds);
    }

    /// Strike the next ready hit into the stretcher. With the pool empty the
    /// last hit is struck again; with nothing rendered yet at all (right
    /// after start-up) the strike is owed until the first hit is ready.
    fn strike(&mut self) {
        let hit = match self.pool.take(self.generation) {
            Some(h) => h,
            None => match &self.last_hit {
                Some(h) => {
                    self.starved += 1;
                    h.clone()
                }
                None => {
                    self.owed = (self.owed + 1).min(2);
                    return;
                }
            },
        };
        self.stretcher.strike(hit.clone());
        self.last_hit = Some(hit);
        self.strikes = self.strikes.wrapping_add(1);
    }

    /// Keep the pool stocked: render a slice of the next hit if one is
    /// needed (or already under way).
    fn feed(&mut self, frames: usize) {
        if let Some(g) = self.renderer.job_generation()
            && g != self.generation
        {
            self.renderer.abort();
        }
        let wanted = self.pool.fresh(self.generation) < self.pool_target;
        if !wanted && !self.renderer.busy() {
            return;
        }
        let speed = if self.pool.is_empty() {
            RENDER_SPEED_URGENT
        } else {
            RENDER_SPEED
        };
        if let Some(hit) = self
            .renderer
            .work(&self.source, self.generation, frames * speed)
        {
            let generation = hit.generation;
            self.pool.push(Rc::new(hit), generation);
            self.plan();
            if self.playing {
                while self.owed > 0 {
                    self.owed -= 1;
                    self.strike();
                }
            }
        }
    }

    /// Render a stereo block (any length; `left` and `right` equally long).
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let mut done = 0;
        while done < n {
            let k = (n - done).min(BLOCK);
            self.render_block(k);
            left[done..done + k].copy_from_slice(&self.scratch[0][..k]);
            right[done..done + k].copy_from_slice(&self.scratch[1][..k]);
            done += k;
        }
    }

    fn render_block(&mut self, n: usize) {
        if self.dirty {
            self.refresh();
        }
        if std::mem::take(&mut self.manual_strike) && self.playing {
            self.strike();
        }
        if self.playing && !self.stretch.freeze {
            if self.strike_now {
                self.strike_now = false;
                self.strike();
            }
            if let Some(step) =
                self.clock
                    .advance(n, self.step_seconds, self.sample_rate, self.pattern.steps)
                && self.pattern.is_pulse(step)
                && self.rng.uniform() < self.chance
            {
                self.strike();
            }
        }
        self.feed(n);

        let [l, r] = &mut self.scratch;
        let (l, r) = (&mut l[..n], &mut r[..n]);
        self.stretcher.render(&self.stretch, l, r);
        let (target, k) = if self.playing {
            (1.0, self.k_start)
        } else {
            (0.0, self.k_release)
        };
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            self.play_gain += (target - self.play_gain) * k;
            *a *= self.play_gain;
            *b *= self.play_gain;
        }
        if !self.playing && self.play_gain < 1e-4 && self.stretcher.layer_count() > 0 {
            self.stretcher.clear();
        }

        self.echo.process(l, r);
        self.plate.process(l, r);

        let master = map::master_gain(self.patch[Param::Master.index()]);
        let mut peak = [0.0f32; 2];
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            self.master += (master - self.master) * self.k_master;
            let g = self.master * OUTPUT_GAIN;
            *a = soft_clip(self.dc[0].process(*a, self.dc_r) * g);
            *b = soft_clip(self.dc[1].process(*b, self.dc_r) * g);
            peak[0] = peak[0].max(a.abs());
            peak[1] = peak[1].max(b.abs());
        }
        for (p, now) in self.peak.iter_mut().zip(peak) {
            *p = now.max(*p * 0.97);
        }
    }

    /// Fill the panel's telemetry.
    pub fn telemetry(&self, out: &mut t::Telemetry) {
        out.fill(0.0);
        out[t::PLAYING] = (self.playing || self.play_gain > 1e-3) as u32 as f32;
        out[t::FROZEN] = self.stretch.freeze as u32 as f32;
        out[t::STEP] = self.clock.step() as f32;
        out[t::STEP_PHASE] = self.clock.phase();
        out[t::STEPS] = self.pattern.steps as f32;
        out[t::MASK] = self.pattern.mask() as f32;
        out[t::STRIKES] = (self.strikes & 0xff_ffff) as f32;
        out[t::POOL] = self.pool.len() as f32;
        out[t::POOL_FRESH] = self.pool.fresh(self.generation) as f32;
        out[t::POOL_TARGET] = self.pool_target as f32;
        out[t::RENDERING] = self.renderer.progress();
        out[t::HIT_AVERAGE] = self.renderer.average_seconds();
        out[t::STARVED] = self.starved as f32;
        out[t::LAYERS] = self.stretcher.layer_count() as f32;
        out[t::PEAK_L] = self.peak[0];
        out[t::PEAK_R] = self.peak[1];
        out[t::HOP_SECONDS] = self.stretcher.hop() as f32 / self.sample_rate;
        out[t::OVERRUNS] = self.stretcher.overruns as f32;
        out[t::PLAY_GAIN] = self.play_gain;
        if let Some(h) = &self.last_hit {
            out[t::HIT_SECONDS] = h.len() as f32 / self.sample_rate;
            out[t::HIT_ALGO_A] = h.algorithms[0] as f32;
            out[t::HIT_ALGO_B] = h.algorithms[1] as f32;
            out[t::HIT_PITCH_A] = h.pitch_hz[0];
            out[t::HIT_PITCH_B] = h.pitch_hz[1];
            for (i, (lo, hi)) in h.outline.iter().enumerate() {
                out[t::OUTLINE_BASE + 2 * i] = *lo;
                out[t::OUTLINE_BASE + 2 * i + 1] = *hi;
            }
        }
        let count = self.stretcher.layer_count();
        for (i, l) in self
            .stretcher
            .layers()
            .skip(count.saturating_sub(MAX_LAYERS))
            .enumerate()
        {
            let base = t::LAYER_BASE + i * t::LAYER_FIELDS;
            out[base] = l.progress;
            out[base + 1] = l.gain;
            out[base + 2] = l.algorithms[0] as f32;
            out[base + 3] = l.algorithms[1] as f32;
        }
        out[t::SPECTRUM_BASE..t::SPECTRUM_BASE + crate::stretch::SPECTRUM_BANDS]
            .copy_from_slice(self.stretcher.spectrum());
        debug_assert_eq!(t::LEN, t::OUTLINE_BASE + 2 * OUTLINE);
    }
}

/// Transparent below −6 dBFS, saturating gently above.
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
    use crate::params::ALL_PARAMS;
    use crate::presets::SCENES;

    const SR: f32 = 48_000.0;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn run(m: &mut Machine, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let frames = (seconds * m.sample_rate()) as usize;
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        for (a, b) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
            m.render(a, b);
        }
        (l, r)
    }

    #[test]
    fn silent_until_played_but_prepares_hits() {
        let mut m = Machine::new(SR, 1);
        let (l, r) = run(&mut m, 3.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0));
        assert!(!m.pool.is_empty(), "pool {}", m.pool.len());
    }

    #[test]
    fn plays_a_drone_and_stops() {
        let mut m = Machine::new(SR, 2);
        run(&mut m, 2.0);
        m.set_playing(true);
        let (l, r) = run(&mut m, 20.0);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.0));
        let body = rms(&l[(4.0 * SR) as usize..]);
        assert!(body > 0.02, "body {body}");
        assert!(m.strikes >= 3, "strikes {}", m.strikes);
        assert_eq!(m.stretcher.overruns, 0);

        m.set_playing(false);
        let mut off = m.patch;
        off[Param::EchoOn.index()] = 0.0;
        off[Param::VerbOn.index()] = 0.0;
        m.load_patch(&off);
        let (l, _) = run(&mut m, 6.0);
        assert!(rms(&l[(5.0 * SR) as usize..]) < 1e-4);
        assert_eq!(m.stretcher.layer_count(), 0);
    }

    #[test]
    fn freeze_stops_new_strikes() {
        let mut m = Machine::new(SR, 3);
        m.set_param(Param::StepTime, 0.0);
        m.set_param(Param::Pulses, 8.0);
        m.set_playing(true);
        run(&mut m, 2.0);
        m.set_param(Param::Freeze, 1.0);
        let before = m.strikes;
        let (l, _) = run(&mut m, 4.0);
        assert_eq!(m.strikes, before);
        assert!(rms(&l[(3.0 * SR) as usize..]) > 0.01);
    }

    #[test]
    fn manual_strikes_work_while_frozen_but_not_stopped() {
        let mut m = Machine::new(SR, 7);
        m.strike_now();
        run(&mut m, 3.0);
        assert_eq!(m.strikes, 0);
        m.set_param(Param::Freeze, 1.0);
        m.set_playing(true);
        run(&mut m, 0.1);
        let before = m.strikes;
        m.strike_now();
        run(&mut m, 0.1);
        assert_eq!(m.strikes, before + 1);
    }

    #[test]
    fn source_changes_refresh_the_pool() {
        let mut m = Machine::new(SR, 4);
        run(&mut m, 4.0);
        let g = m.generation;
        m.set_param(Param::Algorithms, 1.0);
        assert_ne!(m.generation, g);
        run(&mut m, 6.0);
        assert!(m.pool.fresh(m.generation) >= 1);
        // Stretch settings don't invalidate hits.
        let g = m.generation;
        m.set_param(Param::Stretch, 0.9);
        assert_eq!(m.generation, g);
    }

    #[test]
    fn every_scene_plays_cleanly() {
        for (i, scene) in SCENES.iter().enumerate() {
            let mut m = Machine::new(SR, 10 + i as u32);
            m.load_patch(&scene.patch(&default_patch()));
            m.snap();
            m.set_playing(true);
            let (l, r) = run(&mut m, 12.0);
            assert!(
                l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.0),
                "{}",
                scene.name
            );
            assert!(
                rms(&l[(3.0 * SR) as usize..]) > 5e-3,
                "{} too quiet",
                scene.name
            );
            assert_eq!(m.stretcher.overruns, 0, "{}", scene.name);
        }
    }

    #[test]
    fn survives_random_knob_twiddling() {
        let mut m = Machine::new(44_100.0, 5);
        let mut rng = Noise::new(9);
        m.set_playing(true);
        for round in 0..30 {
            for p in ALL_PARAMS {
                if rng.uniform() < 0.3 {
                    let v = p.kind().from_norm(rng.uniform());
                    m.set_param(p, v);
                }
            }
            m.set_param(Param::Algorithms, 1.0 + (rng.uniform() * 254.0).round());
            let (l, r) = run(&mut m, 0.5);
            assert!(
                l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.0),
                "round {round}"
            );
        }
    }

    #[test]
    fn telemetry_reports() {
        let mut m = Machine::new(SR, 6);
        m.set_playing(true);
        run(&mut m, 3.0);
        let mut tel = [0.0; t::LEN];
        m.telemetry(&mut tel);
        assert_eq!(tel[t::PLAYING], 1.0);
        assert!(tel[t::LAYERS] >= 1.0);
        assert!(tel[t::HIT_SECONDS] > 0.0);
        assert_eq!(tel[t::MASK] as u32, 0b0100_1001);
        assert!(
            tel[t::SPECTRUM_BASE..t::SPECTRUM_BASE + 48]
                .iter()
                .any(|&b| b > 0.0)
        );
    }
}
