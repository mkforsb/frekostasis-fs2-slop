//! The headless FK-2: MUTATE, strike both channels, record the result.
//!
//! Every strike is a Frekussion FK-2 with no panel: both channels are
//! mutated, as the FK-2's MUTATE knob does on each hit, then shaped by the
//! SOURCE controls (pitch, scale, length, grit, ratio set, FM index, cross
//! modulation) and struck together, as Space does. Instead of being played
//! the result is rendered offline, a slice per audio block, and watched: once
//! its level has fallen 50 dB below its loudest moment the sound is over. It
//! is trimmed, faded, loudness-normalized and handed to the [`Pool`], where
//! it waits to be stretched.

use std::collections::VecDeque;
use std::rc::Rc;

use frekussion_dsp::params::{
    ALGORITHMS, LFO_PITCH, OpParam, ParamKind as FkKind, RATIOS, default_patch as fk_default,
    map as fk_map, ratio_knob,
};
use frekussion_dsp::presets::KITS;
use frekussion_dsp::{ALL_PARAMS as FK_PARAMS, Engine as Fk2, OPS, Param as FkParam, VoicePatch};

use crate::params::{Param, Patch, RATIOS_HARMONIC, RATIOS_INHARMONIC, SCALE_FREE, map};
use crate::util::Noise;

/// FK-2 channels struck per hit.
pub const FK_VOICES: usize = 2;
/// Longest hit kept; anything still ringing is faded out.
pub const MAX_HIT_SECONDS: f32 = 8.0;
/// Level-detection window, frames.
const WIN: usize = 512;
/// A hit is over once a window is this far (power ratio, −50 dB) below the
/// loudest window.
const END_RATIO: f32 = 1e-5;
/// … or below this absolute power (−100 dBFS).
const END_FLOOR: f32 = 1e-10;
const MIN_HIT_SECONDS: f32 = 0.12;
/// A hit whose loudest window stays below this power (−80 dBFS) by
/// [`SILENT_AFTER_SECONDS`] is thrown away and struck again.
const SILENT_POWER: f32 = 1e-8;
const SILENT_AFTER_SECONDS: f32 = 0.3;
/// Loudness is measured over this many consecutive windows (≈ 85 ms).
const LOUDNESS_WINDOWS: usize = 8;
/// Every hit is normalized so its loudest ≈ 85 ms has this RMS.
pub const TARGET_RMS: f32 = 0.25;
const MAX_NORMALIZE_GAIN: f32 = 200.0;
const END_FADE_SECONDS: f32 = 0.03;
const CUT_FADE_SECONDS: f32 = 0.5;
/// Columns of the waveform outline sent to the panel.
pub const OUTLINE: usize = 48;
/// Carriers are never quieter than this LEVEL knob position after shaping,
/// so an algorithm change can't leave a strike silent.
const MIN_CARRIER_LEVEL: f32 = 0.45;

/// The SOURCE section's controls, as the renderer uses them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceSettings {
    pub mutate: f32,
    /// Bit `a` set: FK-2 algorithm `a` may be picked. Never zero.
    pub algorithms: u32,
    pub origin: usize,
    /// Knob positions (see [`map`]).
    pub pitch: f32,
    pub range: f32,
    pub scale: u32,
    pub root: u32,
    pub length: f32,
    pub grit: f32,
    pub ratios: u32,
    pub index: f32,
    pub cross: f32,
}

impl SourceSettings {
    pub fn from_patch(p: &Patch) -> Self {
        let v = |param: Param| param.sanitize(p[param.index()]);
        Self {
            mutate: v(Param::Mutate),
            algorithms: (v(Param::Algorithms) as u32) & 0xff,
            origin: v(Param::Origin) as usize,
            pitch: v(Param::Pitch),
            range: v(Param::Range),
            scale: v(Param::Scale) as u32,
            root: v(Param::Root) as u32,
            length: v(Param::Length),
            grit: v(Param::Grit),
            ratios: v(Param::Ratios) as u32,
            index: v(Param::Index),
            cross: v(Param::Cross),
        }
    }
}

/// One recorded strike of both FK-2 channels.
#[derive(Clone, Debug)]
pub struct Hit {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// FK-2 algorithm of each channel.
    pub algorithms: [u32; FK_VOICES],
    /// Pitch each channel was struck at.
    pub pitch_hz: [f32; FK_VOICES],
    /// Min/max of the (normalized) mono mix in [`OUTLINE`] columns.
    pub outline: [(f32, f32); OUTLINE],
    /// The source settings' generation the hit was rendered with.
    pub generation: u32,
}

impl Hit {
    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
}

// --- mutation ---------------------------------------------------------------

/// Controls MUTATE never changes, as on the FK-2.
fn keeps_value_on_randomize(p: FkParam) -> bool {
    matches!(
        p,
        FkParam::Level | FkParam::Sense | FkParam::Pan | FkParam::Link
    )
}

fn enabled_algorithms(mask: u32) -> impl Iterator<Item = u32> {
    (0..ALGORITHMS.len() as u32).filter(move |a| mask & (1 << a) != 0)
}

/// Which operator (and control) an FK-2 parameter belongs to.
fn op_role(p: FkParam) -> Option<(usize, OpParam)> {
    (0..OPS).find_map(|op| {
        OpParam::ALL
            .iter()
            .find(|w| w.of(op) == p)
            .map(|&w| (op, w))
    })
}

/// A random FK-2 patch, exactly as the FK-2's RND button draws it (every
/// carrier audible, the effects that can dominate a sound often left out),
/// except that the algorithm is drawn from the `algorithms` mask only.
pub fn random_patch(current: &VoicePatch, rng: &mut Noise, algorithms: u32) -> VoicePatch {
    let algorithms = if algorithms & 0xff == 0 {
        0xff
    } else {
        algorithms
    };
    let mut patch = *current;
    let mut uniform = || rng.sample() * 0.5 + 0.5;
    // Draw every value in a fixed order so a seed always gives one patch.
    let draws: Vec<(f32, f32)> = FK_PARAMS.iter().map(|_| (uniform(), uniform())).collect();
    let n = enabled_algorithms(algorithms).count();
    let pick = ((draws[FkParam::Algorithm.index()].0 * n as f32) as usize).min(n - 1);
    let algo_index = enabled_algorithms(algorithms).nth(pick).unwrap_or(0);
    let algo = ALGORITHMS[algo_index as usize];
    for p in FK_PARAMS {
        if keeps_value_on_randomize(p) {
            continue;
        }
        let (x, coin) = draws[p.index()];
        let sometimes = |value: f32| if coin < 0.5 { value } else { p.default_value() };
        let role = op_role(p).map(|(op, w)| (w, algo.is_carrier(op)));
        patch[p.index()] = p.sanitize(match (p, p.info().kind, role) {
            (FkParam::Algorithm, _, _) => algo_index as f32,
            (_, FkKind::Choice(names), _) => (x * names.len() as f32).floor(),
            (_, _, Some((OpParam::Level, true))) => 0.5 + 0.45 * x,
            (_, _, Some((OpParam::Level, false))) => 0.75 * x,
            (_, _, Some((OpParam::Decay, _))) => 0.15 + 0.65 * x,
            (_, _, Some((OpParam::Fine, _))) => sometimes(0.25 + 0.5 * x),
            (_, _, Some((OpParam::Ratio, _))) => x,
            (FkParam::Tune, _, _) => 0.08 + 0.84 * x,
            (FkParam::Decay, _, _) => 0.15 + 0.55 * x,
            (FkParam::Feedback, _, _) => 0.7 * x * x,
            (FkParam::SweepAmount, _, _) => sometimes(0.15 + 0.8 * x),
            (FkParam::Drive, _, _) => sometimes(0.6 * x),
            (FkParam::Filter, _, _) => sometimes(0.2 + 0.6 * x),
            (FkParam::Resonance | FkParam::NoiseRes, _, _) => 0.6 * x,
            (FkParam::NoiseLevel, _, _) => sometimes(0.3 + 0.6 * x),
            (FkParam::NoiseDecay, _, _) => 0.1 + 0.6 * x,
            (FkParam::NoiseFm, _, _) => sometimes(0.6 * x),
            (FkParam::LfoDepth, _, _) => sometimes(0.6 * x * x),
            (FkParam::XmodAmount, _, _) => sometimes(0.7 * x),
            (FkParam::Ring, _, _) => sometimes(0.5 * x),
            (FkParam::ReverbMix, _, _) => 0.75 * x * x,
            _ => x,
        });
    }
    patch
}

/// FK-2 MUTATE: move every control `amount` of the way towards a fresh
/// random patch. The algorithm can't be interpolated, so it jumps to the
/// random one with probability `amount` (always, if the current algorithm
/// has been switched off in the mask).
pub fn mutate_patch(
    current: &VoicePatch,
    amount: f32,
    rng: &mut Noise,
    algorithms: u32,
) -> VoicePatch {
    let target = random_patch(current, rng, algorithms);
    let algo_roll = rng.uniform();
    if amount >= 1.0 {
        return target;
    }
    let amount = amount.max(0.0);
    let mut patch = *current;
    for p in FK_PARAMS {
        let (c, t) = (current[p.index()], target[p.index()]);
        patch[p.index()] = p.sanitize(c + (t - c) * amount);
    }
    let a = FkParam::Algorithm.index();
    let current_allowed = algorithms & (1 << current[a] as u32) != 0;
    patch[a] = if !current_allowed || algo_roll < amount {
        target[a]
    } else {
        current[a]
    };
    patch
}

/// Is an operator ratio harmonic (a multiple of ¼)?
pub fn is_harmonic_ratio(r: f32) -> bool {
    let q = r * 4.0;
    (q - q.round()).abs() < 1e-3
}

/// The allowed ratio nearest (in log frequency) to `r`.
fn nearest_ratio(r: f32, set: u32) -> f32 {
    RATIOS
        .iter()
        .copied()
        .filter(|&c| match set {
            RATIOS_HARMONIC => is_harmonic_ratio(c),
            RATIOS_INHARMONIC => !is_harmonic_ratio(c),
            _ => true,
        })
        .min_by(|a, b| {
            let da = (a / r).ln().abs();
            let db = (b / r).ln().abs();
            da.total_cmp(&db)
        })
        .unwrap_or(r)
}

/// Apply the SOURCE shaping to a (mutated) patch: pitch drawn around PITCH
/// within RANGE and snapped to SCALE, LENGTH, GRIT, the RATIO set, INDEX,
/// CROSS, and a stereo spread for the channel. Returns the patch and its
/// pitch.
pub fn shape_patch(
    patch: &VoicePatch,
    s: &SourceSettings,
    voice: usize,
    rng: &mut Noise,
) -> (VoicePatch, f32) {
    let mut p = *patch;
    let set = |p: &mut VoicePatch, param: FkParam, v: f32| p[param.index()] = param.sanitize(v);
    let get = |p: &VoicePatch, param: FkParam| p[param.index()];
    let algo = ALGORITHMS[(get(&p, FkParam::Algorithm) as usize).min(ALGORITHMS.len() - 1)];

    let spread = map::range_octaves(s.range) * rng.sample();
    let hz = map::quantize_hz(map::pitch_hz(s.pitch) * spread.exp2(), s.root, s.scale);
    set(&mut p, FkParam::Tune, map::pitch_knob(hz));
    set(&mut p, FkParam::Fine, 0.5);
    if s.scale != SCALE_FREE && get(&p, FkParam::LfoDest) as u32 == LFO_PITCH {
        // Keep snapped pitches in tune: only a little LFO on pitch.
        let d = get(&p, FkParam::LfoDepth);
        set(&mut p, FkParam::LfoDepth, d * 0.25);
    }

    let decay = map::length_decay_knob(s.length) + 0.08 * rng.sample();
    set(&mut p, FkParam::Decay, decay);
    // Carrier envelopes multiply the amp envelope: let them ring at least as
    // long, or LENGTH would do nothing on short-carrier patches.
    let seconds = fk_map::decay_seconds(get(&p, FkParam::Decay));
    let carrier_decay = ((seconds / 0.002).ln() / 6000f32.ln()).clamp(0.0, 1.0);

    let index = map::index_knob_scale(s.index);
    let cross = map::cross_knob_scale(s.cross);
    for op in 0..OPS {
        let level = OpParam::Level.of(op);
        let ratio = OpParam::Ratio.of(op);
        if algo.is_carrier(op) {
            let l = get(&p, level).max(MIN_CARRIER_LEVEL);
            set(&mut p, level, l);
            let d = get(&p, OpParam::Decay.of(op)).max(carrier_decay);
            set(&mut p, OpParam::Decay.of(op), d);
        } else {
            let l = get(&p, level) * index;
            set(&mut p, level, l);
        }
        let r = nearest_ratio(fk_map::op_ratio(get(&p, ratio)), s.ratios);
        set(&mut p, ratio, ratio_knob(r));
    }

    let ceiling = map::grit_ceiling(s.grit);
    for g in [FkParam::Drive, FkParam::NoiseFm] {
        let v = get(&p, g).min(ceiling);
        set(&mut p, g, v);
    }
    let fb = get(&p, FkParam::Feedback).min(map::feedback_ceiling(s.grit));
    set(&mut p, FkParam::Feedback, fb);

    for x in [FkParam::XmodAmount, FkParam::Ring] {
        let v = get(&p, x) * cross;
        set(&mut p, x, v);
    }
    // Both channels always strike together here.
    set(&mut p, FkParam::Link, 0.0);

    // Channel 1 leans left, channel 2 right.
    let side = if voice == 0 { -1.0 } else { 1.0 };
    set(
        &mut p,
        FkParam::Pan,
        0.5 + side * (0.1 + 0.3 * rng.uniform()),
    );
    (p, hz)
}

// --- rendering -------------------------------------------------------------------

#[derive(Debug)]
struct Job {
    engine: Box<Fk2>,
    generation: u32,
    frames: usize,
    win_acc: f32,
    win_fill: usize,
    /// Powers of the last [`LOUDNESS_WINDOWS`] windows (ring).
    recent: [f32; LOUDNESS_WINDOWS],
    recent_pos: usize,
    /// Loudest single window and loudest ≈ 85 ms stretch, as mean power.
    peak_window: f32,
    peak_loudness: f32,
    algorithms: [u32; FK_VOICES],
    pitch_hz: [f32; FK_VOICES],
}

/// A rendered strike being trimmed, faded, normalized and copied out of the
/// scratch buffers, a slice at a time.
#[derive(Debug)]
struct Finish {
    left: Vec<f32>,
    right: Vec<f32>,
    frames: usize,
    fade: usize,
    gain: f32,
    outline: [(f32, f32); OUTLINE],
    per_column: usize,
    algorithms: [u32; FK_VOICES],
    pitch_hz: [f32; FK_VOICES],
    generation: u32,
}

/// Samples finished per rendered frame of budget (copying is much cheaper
/// than synthesis).
const FINISH_SPEED: usize = 8;

/// Renders FK-2 strikes a slice at a time.
#[derive(Debug)]
pub struct HitRenderer {
    sample_rate: f32,
    rng: Noise,
    /// The channels' patches: MUTATE walks from strike to strike, as on the
    /// FK-2.
    patches: [VoicePatch; FK_VOICES],
    origin: Option<usize>,
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
    job: Option<Job>,
    finish: Option<Finish>,
    /// Running average hit length, seconds.
    avg_seconds: f32,
    /// Hits thrown away for being silent.
    pub discarded: u32,
}

impl HitRenderer {
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let max = Self::max_frames_for(sample_rate);
        Self {
            sample_rate,
            rng: Noise::new(seed),
            patches: [fk_default(); FK_VOICES],
            origin: None,
            scratch_l: vec![0.0; max],
            scratch_r: vec![0.0; max],
            job: None,
            finish: None,
            avg_seconds: 2.0,
            discarded: 0,
        }
    }

    fn max_frames_for(sample_rate: f32) -> usize {
        ((MAX_HIT_SECONDS * sample_rate) as usize).div_ceil(WIN) * WIN
    }

    fn max_frames(&self) -> usize {
        self.scratch_l.len()
    }

    pub fn busy(&self) -> bool {
        self.job.is_some() || self.finish.is_some()
    }

    /// Generation of the hit being rendered, if any.
    pub fn job_generation(&self) -> Option<u32> {
        match (&self.job, &self.finish) {
            (Some(j), _) => Some(j.generation),
            (_, Some(f)) => Some(f.generation),
            _ => None,
        }
    }

    /// Rough progress of the current render (0 when idle).
    pub fn progress(&self) -> f32 {
        match (&self.job, &self.finish) {
            (Some(j), _) => {
                (j.frames as f32 / (self.avg_seconds * self.sample_rate).max(1.0)).min(0.95)
            }
            (_, Some(_)) => 0.98,
            _ => 0.0,
        }
    }

    /// Running average length of rendered hits, seconds.
    pub fn average_seconds(&self) -> f32 {
        self.avg_seconds
    }

    /// Drop the hit being rendered.
    pub fn abort(&mut self) {
        self.job = None;
        self.finish = None;
    }

    fn start(&mut self, s: &SourceSettings, generation: u32) {
        if self.origin != Some(s.origin) {
            self.patches = KITS
                .get(s.origin)
                .map(|k| k.patches())
                .unwrap_or([fk_default(); FK_VOICES]);
            self.origin = Some(s.origin);
        }
        let mut engine = Box::new(Fk2::new(self.sample_rate));
        let mut algorithms = [0; FK_VOICES];
        let mut pitch_hz = [0.0; FK_VOICES];
        for v in 0..FK_VOICES {
            self.patches[v] = mutate_patch(&self.patches[v], s.mutate, &mut self.rng, s.algorithms);
            let (shaped, hz) = shape_patch(&self.patches[v], s, v, &mut self.rng);
            engine.load_patch(v, &shaped);
            algorithms[v] = FkParam::Algorithm.sanitize(shaped[FkParam::Algorithm.index()]) as u32;
            pitch_hz[v] = hz;
        }
        engine.snap_params();
        for v in 0..FK_VOICES {
            engine.trigger(v, 0.75 + 0.25 * self.rng.uniform());
        }
        self.job = Some(Job {
            engine,
            generation,
            frames: 0,
            win_acc: 0.0,
            win_fill: 0,
            recent: [0.0; LOUDNESS_WINDOWS],
            recent_pos: 0,
            peak_window: 0.0,
            peak_loudness: 0.0,
            algorithms,
            pitch_hz,
        });
    }

    /// Render about `frames` more frames of the current strike, striking a
    /// new one first if idle. Returns the hit once it has ended and been
    /// finished.
    pub fn work(&mut self, s: &SourceSettings, generation: u32, frames: usize) -> Option<Hit> {
        if self.finish.is_none() {
            if self.job.is_none() {
                self.start(s, generation);
            }
            self.render(frames);
            return None;
        }
        let f = self.finish.as_mut()?;
        let start = f.left.len();
        let end = (start + frames * FINISH_SPEED).min(f.frames);
        for i in start..end {
            let to_end = f.frames - i;
            let g = if to_end <= f.fade {
                let x = to_end as f32 / f.fade as f32;
                f.gain * x * x
            } else {
                f.gain
            };
            let (l, r) = (self.scratch_l[i] * g, self.scratch_r[i] * g);
            f.left.push(l);
            f.right.push(r);
            let m = 0.5 * (l + r);
            let o = &mut f.outline[(i / f.per_column).min(OUTLINE - 1)];
            *o = (o.0.min(m), o.1.max(m));
        }
        if end < f.frames {
            return None;
        }
        let f = self.finish.take()?;
        let seconds = f.frames as f32 / self.sample_rate;
        self.avg_seconds += (seconds - self.avg_seconds) * 0.3;
        Some(Hit {
            left: f.left,
            right: f.right,
            algorithms: f.algorithms,
            pitch_hz: f.pitch_hz,
            outline: f.outline,
            generation: f.generation,
        })
    }

    /// Synthesize and watch the level; hands the job to [`Finish`] once the
    /// sound is over.
    fn render(&mut self, frames: usize) {
        let sr = self.sample_rate;
        let max = self.max_frames();
        let Some(job) = self.job.as_mut() else { return };
        let mut left = frames;
        while left > 0 {
            // Render up to the end of the current detection window.
            let n = left.min(WIN - job.win_fill).min(max - job.frames);
            let (a, b) = (job.frames, job.frames + n);
            job.engine
                .render(&mut self.scratch_l[a..b], &mut self.scratch_r[a..b]);
            job.win_acc += self.scratch_l[a..b]
                .iter()
                .zip(&self.scratch_r[a..b])
                .map(|(l, r)| 0.5 * (l * l + r * r))
                .sum::<f32>();
            job.win_fill += n;
            job.frames = b;
            left -= n;
            if job.win_fill < WIN {
                continue;
            }
            let power = job.win_acc / WIN as f32;
            job.win_acc = 0.0;
            job.win_fill = 0;
            job.recent[job.recent_pos] = power;
            job.recent_pos = (job.recent_pos + 1) % LOUDNESS_WINDOWS;
            job.peak_window = job.peak_window.max(power);
            let loudness = job.recent.iter().sum::<f32>() / LOUDNESS_WINDOWS as f32;
            job.peak_loudness = job.peak_loudness.max(loudness);

            let seconds = job.frames as f32 / sr;
            if seconds >= SILENT_AFTER_SECONDS && job.peak_window < SILENT_POWER {
                self.job = None;
                self.discarded += 1;
                return;
            }
            let ended = seconds >= MIN_HIT_SECONDS
                && (power < job.peak_window * END_RATIO || power < END_FLOOR);
            if ended || job.frames >= max {
                let n = job.frames;
                let fade = (if ended {
                    END_FADE_SECONDS
                } else {
                    CUT_FADE_SECONDS
                } * sr) as usize;
                self.finish = Some(Finish {
                    left: Vec::with_capacity(n),
                    right: Vec::with_capacity(n),
                    frames: n,
                    fade: fade.clamp(1, n),
                    gain: (TARGET_RMS / job.peak_loudness.max(1e-12).sqrt())
                        .min(MAX_NORMALIZE_GAIN),
                    outline: [(0.0, 0.0); OUTLINE],
                    per_column: n.div_ceil(OUTLINE).max(1),
                    algorithms: job.algorithms,
                    pitch_hz: job.pitch_hz,
                    generation: job.generation,
                });
                self.job = None;
                return;
            }
        }
    }
}

// --- pool -----------------------------------------------------------------------------

/// Rendered hits waiting to be struck into the stretcher.
///
/// When the SOURCE controls change, the hits already here are out of date
/// but are still better than silence: they are kept (and used if nothing
/// newer is ready) until fresh ones replace them.
#[derive(Debug)]
pub struct Pool {
    hits: VecDeque<Rc<Hit>>,
}

/// Upper bound on the pool size (fresh + stale).
pub const POOL_MAX: usize = 8;

impl Default for Pool {
    fn default() -> Self {
        Self {
            hits: VecDeque::with_capacity(POOL_MAX + 1),
        }
    }
}

impl Pool {
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    pub fn fresh(&self, generation: u32) -> usize {
        self.hits
            .iter()
            .filter(|h| h.generation == generation)
            .count()
    }

    /// Add a fresh hit, dropping the oldest stale one (or, if full, the
    /// oldest of all) to make room.
    pub fn push(&mut self, hit: Rc<Hit>, generation: u32) {
        if let Some(i) = self.hits.iter().position(|h| h.generation != generation) {
            self.hits.remove(i);
        }
        while self.hits.len() >= POOL_MAX {
            self.hits.pop_front();
        }
        self.hits.push_back(hit);
    }

    /// The oldest fresh hit, else the newest stale one.
    pub fn take(&mut self, generation: u32) -> Option<Rc<Hit>> {
        if let Some(i) = self.hits.iter().position(|h| h.generation == generation) {
            return self.hits.remove(i);
        }
        self.hits.pop_back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::default_patch;

    fn settings() -> SourceSettings {
        SourceSettings::from_patch(&default_patch())
    }

    fn render_one(r: &mut HitRenderer, s: &SourceSettings) -> Hit {
        for _ in 0..10_000 {
            if let Some(h) = r.work(s, 1, 4096) {
                return h;
            }
        }
        panic!("no hit");
    }

    fn algo(p: &VoicePatch) -> usize {
        p[FkParam::Algorithm.index()] as usize
    }

    #[test]
    fn algorithm_mask_is_respected() {
        let mut rng = Noise::new(1);
        let mut patch = fk_default();
        let mask = (1 << 3) | (1 << 5);
        for _ in 0..200 {
            patch = mutate_patch(&patch, 1.0, &mut rng, mask);
            assert!(algo(&patch) == 3 || algo(&patch) == 5, "{}", algo(&patch));
        }
        // Partial mutation leaves a disallowed algorithm immediately.
        let mut p = fk_default();
        p[FkParam::Algorithm.index()] = 0.0;
        let m = mutate_patch(&p, 0.01, &mut rng, 1 << 6);
        assert_eq!(algo(&m), 6);
    }

    #[test]
    fn mutate_off_keeps_the_patch() {
        let mut rng = Noise::new(2);
        let p = KITS[3].patches()[0];
        assert_eq!(mutate_patch(&p, 0.0, &mut rng, 0xff), p);
    }

    #[test]
    fn random_patches_always_have_an_audible_carrier() {
        let mut rng = Noise::new(5);
        for _ in 0..500 {
            let p = random_patch(&fk_default(), &mut rng, 0xff);
            let a = ALGORITHMS[algo(&p)];
            for op in 0..OPS {
                if a.is_carrier(op) {
                    assert!(p[OpParam::Level.of(op).index()] >= 0.5);
                }
            }
        }
    }

    #[test]
    fn harmonic_ratios() {
        assert!(is_harmonic_ratio(0.25) && is_harmonic_ratio(3.5) && is_harmonic_ratio(16.0));
        assert!(!is_harmonic_ratio(1.41) && !is_harmonic_ratio(core::f32::consts::PI));
        assert_eq!(nearest_ratio(1.41, RATIOS_HARMONIC), 1.5);
        assert_eq!(nearest_ratio(2.0, RATIOS_INHARMONIC), 2.14);
        assert_eq!(nearest_ratio(2.3, crate::params::RATIOS_ALL), 2.3);
    }

    #[test]
    fn shaping_applies_every_source_control() {
        let mut s = settings();
        s.grit = 0.1;
        s.scale = 1; // octaves of A
        s.root = 9;
        s.ratios = RATIOS_HARMONIC;
        s.index = 0.0;
        s.cross = 0.0;
        let mut rng = Noise::new(5);
        for _ in 0..200 {
            let random = random_patch(&fk_default(), &mut rng, 0xff);
            let (p, hz) = shape_patch(&random, &s, 0, &mut rng);
            let octaves = (hz / 55.0).log2();
            assert!((octaves - octaves.round()).abs() < 1e-3, "{hz}");
            assert!(p[FkParam::Drive.index()] <= 0.1 && p[FkParam::NoiseFm.index()] <= 0.1);
            assert!(p[FkParam::Feedback.index()] <= map::feedback_ceiling(0.1));
            assert_eq!(p[FkParam::XmodAmount.index()], 0.0);
            assert_eq!(p[FkParam::Ring.index()], 0.0);
            assert_eq!(p[FkParam::Link.index()], 0.0);
            assert!(p[FkParam::Pan.index()] < 0.5);
            let a = ALGORITHMS[algo(&p)];
            for op in 0..OPS {
                let r = fk_map::op_ratio(p[OpParam::Ratio.of(op).index()]);
                assert!(is_harmonic_ratio(r), "{r}");
                let level = p[OpParam::Level.of(op).index()];
                if a.is_carrier(op) {
                    assert!(level >= MIN_CARRIER_LEVEL);
                } else {
                    assert_eq!(level, 0.0, "INDEX 0 turns FM off");
                }
            }
        }
    }

    #[test]
    fn hits_end_are_normalized_and_bounded() {
        let sr = 48_000.0;
        let mut r = HitRenderer::new(sr, 42);
        let s = settings();
        for _ in 0..12 {
            let h = render_one(&mut r, &s);
            assert!(h.len() >= (MIN_HIT_SECONDS * sr) as usize);
            assert!(h.len() <= r.max_frames());
            assert!(
                h.left
                    .iter()
                    .chain(&h.right)
                    .all(|x| x.is_finite() && x.abs() < 8.0)
            );
            let win = WIN * LOUDNESS_WINDOWS;
            let loudest = h
                .left
                .chunks(WIN)
                .zip(h.right.chunks(WIN))
                .map(|(l, r)| {
                    l.iter()
                        .zip(r)
                        .map(|(a, b)| 0.5 * (a * a + b * b))
                        .sum::<f32>()
                })
                .collect::<Vec<_>>()
                .windows(LOUDNESS_WINDOWS)
                .map(|w| (w.iter().sum::<f32>() / win as f32).sqrt())
                .fold(0.0, f32::max);
            assert!((loudest / TARGET_RMS - 1.0).abs() < 0.2, "{loudest}");
            let tail = &h.left[h.len() - 64..];
            assert!(tail.iter().all(|x| x.abs() < 0.05));
        }
    }

    /// Whatever MUTATE rolls and however the SOURCE controls are set, a
    /// strike is audible and bounded.
    #[test]
    fn shaped_random_strikes_are_playable() {
        let fs = 24_000.0;
        let mut rng = Noise::new(2024);
        let mut algos = [0usize; 8];
        for i in 0..300 {
            let mut s = settings();
            s.index = rng.uniform();
            s.cross = rng.uniform();
            s.grit = rng.uniform();
            s.ratios = (rng.uniform() * 3.0) as u32;
            let mut e = Fk2::new(fs);
            for v in 0..FK_VOICES {
                let random = random_patch(&fk_default(), &mut rng, 0xff);
                let (p, _) = shape_patch(&random, &s, v, &mut rng);
                algos[algo(&p)] += 1;
                e.load_patch(v, &p);
            }
            e.snap_params();
            e.trigger(0, 0.9);
            e.trigger(1, 0.9);
            let (l, r) = e.render_vec((fs * 0.3) as usize);
            let peak = l.iter().chain(&r).fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "strike {i}");
            assert!(peak > 2e-3 && peak <= 1.0, "strike {i}: peak {peak}");
        }
        assert!(algos.iter().all(|&n| n > 30), "algorithm spread {algos:?}");
    }

    #[test]
    fn long_hits_are_cut_at_the_limit() {
        let sr = 16_000.0;
        let mut r = HitRenderer::new(sr, 7);
        let mut s = settings();
        s.length = 1.0;
        s.mutate = 0.0;
        let h = render_one(&mut r, &s);
        assert!(h.len() <= r.max_frames());
    }

    #[test]
    fn pool_prefers_fresh_hits() {
        let hit = |g: u32| {
            Rc::new(Hit {
                left: vec![0.0; 4],
                right: vec![0.0; 4],
                algorithms: [0; 2],
                pitch_hz: [0.0; 2],
                outline: [(0.0, 0.0); OUTLINE],
                generation: g,
            })
        };
        let mut pool = Pool::default();
        pool.push(hit(1), 1);
        pool.push(hit(1), 1);
        assert_eq!(pool.fresh(2), 0);
        pool.push(hit(2), 2);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.take(2).unwrap().generation, 2);
        assert_eq!(pool.take(2).unwrap().generation, 1);
        assert!(pool.take(2).is_none());
        for _ in 0..20 {
            pool.push(hit(3), 3);
        }
        assert_eq!(pool.len(), POOL_MAX);
    }
}
