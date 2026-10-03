//! Extreme time stretching after Nasca Octavian Paul's PaulStretch.
//!
//! PaulStretch takes a long window of the input, keeps only the magnitude of
//! its spectrum, gives every bin a random phase and overlap-adds the result.
//! The input position advances by `hop / stretch` per output hop of half a
//! window, so a one-second sound at 50x lasts fifty seconds, smeared into a
//! shimmering cloud of its own spectrum.
//!
//! Here several stretched hits (*layers*) play at once, each at its own
//! input position. Windowing and FFT are linear, and the phases are thrown
//! away anyway, so the layers' windows are **summed before the transform**:
//! a frame costs one forward and one inverse FFT however many layers there
//! are. Stereo goes through a single complex FFT (left in the real part,
//! right in the imaginary part), separated and re-packed around the random
//! phases.
//!
//! Each frame goes through a pipeline of stages (gather, forward FFT, the
//! spectral shaping passes, random phases, inverse FFT, overlap-add). The
//! pipeline for the *next* hop is worked on a bounded slice at a time while
//! the current hop plays, so the audio thread never has to find several
//! milliseconds for a 65 536-point FFT in one block.

use std::rc::Rc;

use crate::util::Noise;

use crate::fft::{Complex, FftJob, FftTables};
use crate::params::{Param, Patch, WINDOW_MAX_LOG2, WINDOW_MIN_LOG2, map};
use crate::source::Hit;

/// Layers that may sound at once; beyond this the oldest fade out.
pub const MAX_LAYERS: usize = 24;
/// Log-spaced bands of the spectrum shown on the panel.
pub const SPECTRUM_BANDS: usize = 48;
const SPECTRUM_LO_HZ: f32 = 30.0;
const SPECTRUM_HI_HZ: f32 = 16_000.0;
const PHASES: usize = 4096;
/// A new layer fades in over this long (its attack is smeared by the
/// window anyway; this keeps the entry soft at small windows).
const LAYER_ATTACK_SECONDS: f32 = 1.5;
/// An evicted layer fades out over this long.
const EVICT_SECONDS: f32 = 3.0;
/// The layered output is levelled towards this RMS, slowly.
const AGC_TARGET_RMS: f32 = 0.18;
const AGC_MAX_GAIN: f32 = 4.0;
const AGC_MIN_GAIN: f32 = 1.0 / 32.0;
const AGC_ATTACK_SECONDS: f32 = 1.0;
const AGC_RELEASE_SECONDS: f32 = 6.0;
/// Aim to finish a frame in this fraction of a hop.
const PACE: f64 = 0.7;
/// Rumble cut: nothing below LO, full level from HI.
const RUMBLE_LO_HZ: f32 = 18.0;
const RUMBLE_HI_HZ: f32 = 40.0;
/// Width of the envelope FOCUS compares against, octaves.
const FOCUS_ENVELOPE_OCTAVES: f32 = 0.5;

/// Stretch and spectral controls, sampled once per frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StretchSettings {
    pub factor: f32,
    pub log2n: u32,
    pub width: f32,
    pub freeze: bool,
    pub shift_semitones: f32,
    pub sub: f32,
    pub smear_octaves: f32,
    pub focus: f32,
    pub lock: f32,
    pub tilt_db: f32,
    pub root: u32,
}

impl StretchSettings {
    pub fn from_patch(p: &Patch) -> Self {
        let v = |param: Param| param.sanitize(p[param.index()]);
        Self {
            factor: map::stretch_factor(v(Param::Stretch)),
            log2n: map::window_log2(v(Param::Window)),
            width: v(Param::Width),
            freeze: v(Param::Freeze) > 0.5,
            shift_semitones: v(Param::Shift),
            sub: v(Param::Sub),
            smear_octaves: map::smear_octaves(v(Param::Smear)),
            focus: map::focus_exponent(v(Param::Focus)),
            lock: v(Param::Lock),
            tilt_db: map::tilt_db_per_octave(v(Param::Tilt)),
            root: v(Param::Root) as u32,
        }
    }
}

impl Default for StretchSettings {
    fn default() -> Self {
        Self::from_patch(&crate::params::default_patch())
    }
}

#[derive(Clone, Debug)]
struct Layer {
    hit: Rc<Hit>,
    /// Window centre in source samples.
    center: f64,
    gain: f32,
    /// Gain change per hop: positive while fading in, negative once evicted.
    ramp: f32,
    evicted: bool,
    /// Centre and gain for the frame being built.
    frame_center: i64,
    frame_gain: f32,
}

/// What the panel shows of a layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerView {
    /// How far through its source the layer has read, 0..1.
    pub progress: f32,
    pub gain: f32,
    pub algorithms: [u32; 2],
}

/// The pipeline building one frame. Every stage can stop part-way and resume.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Clear {
        t: usize,
    },
    Gather {
        layer: usize,
        t: usize,
    },
    Forward(FftJobState),
    Analyze {
        k: usize,
    },
    Transpose {
        k: usize,
    },
    SmearTable {
        k: usize,
    },
    /// Passes: left forwards, left backwards, right forwards, right backwards.
    Smear {
        pass: usize,
        k: usize,
    },
    FocusEnv {
        k: usize,
    },
    FocusBlur {
        pass: usize,
        k: usize,
    },
    FocusApply {
        k: usize,
    },
    GainTable {
        k: usize,
    },
    Colour {
        k: usize,
    },
    Level,
    Phases {
        k: usize,
    },
    Inverse(FftJobState),
    Synth {
        t: usize,
    },
    Done,
}

/// `FftJob` isn't `PartialEq`; wrap it so `Stage` can be.
#[derive(Clone, Copy, Debug)]
struct FftJobState(FftJob);

impl PartialEq for FftJobState {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

pub struct Stretcher {
    sample_rate: f32,
    tables: FftTables,
    /// Analysis/synthesis window, amplitude correction for the random-phase
    /// overlap, and level compensation, per FFT size.
    windows: Vec<Vec<f32>>,
    hinv: Vec<Vec<f32>>,
    compensation: Vec<f32>,
    phasors: Vec<Complex>,
    rng: Noise,

    buf: Vec<Complex>,
    mag: [Vec<f32>; 2],
    tmp: [Vec<f32>; 2],
    env: Vec<f32>,
    /// Per FFT size: FOCUS's envelope blur coefficients and each bin's
    /// panel spectrum band (`u8::MAX` for none).
    focus_coeff: Vec<Vec<f32>>,
    band_of: Vec<Vec<u8>>,
    /// SMEAR blur coefficients and TILT/LOCK gains, rebuilt (a slice at a
    /// time) only when their controls change.
    smear_coeff: Vec<f32>,
    smear_key: Option<(u32, u32)>,
    gains: Vec<f32>,
    gain_key: Option<(u32, u32, u32, u32)>,
    blur_y: f32,
    frame_energy: f32,
    frame_bands: [f32; SPECTRUM_BANDS],

    /// The hop being played, the next one being built, and the second
    /// halves (already windowed) waiting to overlap them.
    out: [Vec<f32>; 2],
    next_out: [Vec<f32>; 2],
    tail: [Vec<f32>; 2],
    next_tail: [Vec<f32>; 2],
    /// FFT size the current tail belongs to.
    tail_log2n: u32,
    hop: usize,
    read: usize,

    layers: Vec<Layer>,
    pending: Vec<Rc<Hit>>,

    stage: Stage,
    /// Settings, size and silence of the frame being built.
    job: StretchSettings,
    job_log2n: u32,
    job_silent: bool,
    /// Work units granted per output frame for the current job, and the
    /// unspent (or overspent) balance.
    units_per_frame: f64,
    credit: f64,
    /// Frames finished synchronously because the budget fell short.
    pub overruns: u32,

    agc_power: f32,
    agc_gain: f32,
    spectrum: [f32; SPECTRUM_BANDS],
}

impl Stretcher {
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let max_n = 1usize << WINDOW_MAX_LOG2;
        let sizes = WINDOW_MIN_LOG2..=WINDOW_MAX_LOG2;
        let windows: Vec<Vec<f32>> = sizes
            .clone()
            .map(|l| {
                let n = 1usize << l;
                (0..n)
                    .map(|t| {
                        let x = 2.0 * t as f32 / n as f32 - 1.0;
                        (1.0 - x * x).max(0.0).powf(1.25)
                    })
                    .collect()
            })
            .collect();
        // Random-phase frames are uncorrelated, so overlapping halves add in
        // power: divide by the RMS of the two overlapping window values.
        let hinv = windows
            .iter()
            .map(|w| {
                let h = w.len() / 2;
                (0..h)
                    .map(|t| 1.0 / (w[t] * w[t] + w[t + h] * w[t + h]).sqrt().max(1e-3))
                    .collect()
            })
            .collect();
        let compensation = windows
            .iter()
            .map(|w| 1.0 / (w.iter().map(|x| x * x).sum::<f32>() / w.len() as f32).sqrt())
            .collect();
        let phasors = (0..PHASES)
            .map(|i| {
                let a = core::f32::consts::TAU * i as f32 / PHASES as f32;
                Complex::new(a.cos(), a.sin())
            })
            .collect();
        let focus_coeff = sizes
            .clone()
            .map(|l| {
                (0..=(1usize << l) / 2)
                    .map(|k| blur_coeff(k, FOCUS_ENVELOPE_OCTAVES))
                    .collect()
            })
            .collect();
        let log_lo = SPECTRUM_LO_HZ.ln();
        let band_scale = SPECTRUM_BANDS as f32 / (SPECTRUM_HI_HZ.ln() - log_lo);
        let band_of = sizes
            .clone()
            .map(|l| {
                let n = 1usize << l;
                (0..=n / 2)
                    .map(|k| {
                        let f = k as f32 * sample_rate / n as f32;
                        let b = ((f.max(1.0).ln() - log_lo) * band_scale).floor();
                        if f >= SPECTRUM_LO_HZ && (b as usize) < SPECTRUM_BANDS {
                            b as u8
                        } else {
                            u8::MAX
                        }
                    })
                    .collect()
            })
            .collect();
        let half = max_n / 2 + 1;
        let stereo = |len: usize| [vec![0.0; len], vec![0.0; len]];
        Self {
            sample_rate,
            tables: FftTables::new(WINDOW_MAX_LOG2),
            windows,
            hinv,
            compensation,
            phasors,
            rng: Noise::new(seed),
            buf: vec![Complex::ZERO; max_n],
            mag: stereo(half),
            tmp: stereo(half),
            env: vec![0.0; half],
            focus_coeff,
            band_of,
            smear_coeff: vec![0.0; half],
            smear_key: None,
            gains: vec![1.0; half],
            gain_key: None,
            blur_y: 0.0,
            frame_energy: 0.0,
            frame_bands: [0.0; SPECTRUM_BANDS],
            out: stereo(max_n / 2),
            next_out: stereo(max_n / 2),
            tail: stereo(max_n / 2),
            next_tail: stereo(max_n / 2),
            tail_log2n: WINDOW_MIN_LOG2,
            hop: 0,
            read: 0,
            layers: Vec::with_capacity(MAX_LAYERS + 8),
            pending: Vec::with_capacity(8),
            stage: Stage::Done,
            job: StretchSettings::default(),
            job_log2n: WINDOW_MIN_LOG2,
            job_silent: true,
            units_per_frame: 0.0,
            credit: 0.0,
            overruns: 0,
            agc_power: 0.0,
            agc_gain: 1.0,
            spectrum: [0.0; SPECTRUM_BANDS],
        }
    }

    /// Queue a hit; it becomes a layer at the next frame.
    pub fn strike(&mut self, hit: Rc<Hit>) {
        if self.pending.len() < self.pending.capacity() {
            self.pending.push(hit);
        }
    }

    /// Silence everything and forget all layers.
    pub fn clear(&mut self) {
        self.layers.clear();
        self.pending.clear();
        for ch in 0..2 {
            for v in [
                &mut self.out,
                &mut self.next_out,
                &mut self.tail,
                &mut self.next_tail,
            ] {
                v[ch].iter_mut().for_each(|x| *x = 0.0);
            }
        }
        self.stage = Stage::Done;
        self.job_silent = true;
        self.read = self.hop;
        self.agc_power = 0.0;
        self.spectrum = [0.0; SPECTRUM_BANDS];
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn layers(&self) -> impl Iterator<Item = LayerView> + '_ {
        self.layers.iter().map(|l| LayerView {
            progress: (l.center / l.hit.len().max(1) as f64).clamp(0.0, 1.0) as f32,
            gain: l.gain,
            algorithms: l.hit.algorithms,
        })
    }

    /// Band levels of the latest frame, 0..1 (≈ −72 … 0 dB).
    pub fn spectrum(&self) -> &[f32; SPECTRUM_BANDS] {
        &self.spectrum
    }

    /// Current output hop, in frames.
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Render the stretched stream (overwrites `left`/`right`), working on
    /// the next frame as the budget allows.
    pub fn render(&mut self, settings: &StretchSettings, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let mut i = 0;
        while i < n {
            if self.read >= self.hop {
                self.boundary(settings);
            }
            let k = (n - i).min(self.hop - self.read);
            left[i..i + k].copy_from_slice(&self.out[0][self.read..self.read + k]);
            right[i..i + k].copy_from_slice(&self.out[1][self.read..self.read + k]);
            self.read += k;
            i += k;
        }
        self.credit += n as f64 * self.units_per_frame;
        while self.credit > 0.0 && self.stage != Stage::Done {
            let used = self.step(self.credit.ceil() as usize);
            self.credit -= used as f64;
        }
        if self.stage == Stage::Done {
            self.credit = self.credit.min(0.0);
        }
    }

    /// A hop has been played: make the finished frame current and start on
    /// the next one.
    fn boundary(&mut self, settings: &StretchSettings) {
        if self.stage != Stage::Done {
            self.overruns += 1;
            while self.stage != Stage::Done {
                self.step(usize::MAX);
            }
        }
        if self.hop == 0 {
            // First hop: play silence while the first frame is built at the
            // requested size.
            self.job_log2n = settings.log2n.clamp(WINDOW_MIN_LOG2, WINDOW_MAX_LOG2);
            self.tail_log2n = self.job_log2n;
        } else {
            std::mem::swap(&mut self.out, &mut self.next_out);
            std::mem::swap(&mut self.tail, &mut self.next_tail);
            self.tail_log2n = self.job_log2n;
        }
        self.hop = (1usize << self.job_log2n) / 2;
        self.read = 0;
        self.start_job(settings);
    }

    fn start_job(&mut self, s: &StretchSettings) {
        let log2n = s.log2n.clamp(WINDOW_MIN_LOG2, WINDOW_MAX_LOG2);
        let n = 1usize << log2n;
        let h = n / 2;
        let hop_seconds = h as f32 / self.sample_rate;

        for hit in self.pending.drain(..) {
            self.layers.push(Layer {
                hit,
                center: 0.0,
                gain: 0.0,
                ramp: (hop_seconds / LAYER_ATTACK_SECONDS).min(1.0),
                evicted: false,
                frame_center: 0,
                frame_gain: 0.0,
            });
        }
        // Finished: read past the end, or faded out after eviction.
        let half = (n / 2) as f64;
        self.layers
            .retain(|l| l.center - half < l.hit.len() as f64 && !(l.evicted && l.gain <= 0.0));
        let live = self.layers.iter().filter(|l| !l.evicted).count();
        let mut excess = live.saturating_sub(MAX_LAYERS);
        for l in self.layers.iter_mut() {
            if excess == 0 {
                break;
            }
            if !l.evicted {
                l.evicted = true;
                l.ramp = -(hop_seconds / EVICT_SECONDS).min(1.0);
                excess -= 1;
            }
        }

        let advance = h as f64 / s.factor.max(1.0) as f64;
        let mut gather_units = 0usize;
        for l in self.layers.iter_mut() {
            l.frame_center = l.center.round() as i64;
            l.frame_gain = l.gain;
            let (a, b) = overlap(l.frame_center, n, l.hit.len());
            gather_units += b.saturating_sub(a);
            if !s.freeze {
                l.center += advance;
            }
            l.gain = (l.gain + l.ramp).clamp(0.0, 1.0);
            if !l.evicted && l.gain >= 1.0 {
                l.ramp = 0.0;
            }
        }

        self.job = *s;
        self.job_log2n = log2n;
        self.job_silent = self.layers.iter().all(|l| l.frame_gain <= 0.0);
        if self.job_silent {
            self.agc_power *= (-hop_seconds / AGC_RELEASE_SECONDS).exp();
            self.spectrum.iter_mut().for_each(|b| *b *= 0.8);
        }
        let units = if self.job_silent {
            n
        } else {
            n + gather_units + 2 * FftJob::cost(log2n) + 16 * (h + 1) + n
        };
        // The frame must be ready when the hop now playing ends.
        self.units_per_frame = units as f64 / (self.hop.max(1) as f64 * PACE);
        self.stage = if self.job_silent {
            Stage::Synth { t: 0 }
        } else {
            Stage::Clear { t: 0 }
        };
    }

    /// Do up to about `budget` units of the current frame. Returns units used.
    fn step(&mut self, budget: usize) -> usize {
        let log2n = self.job_log2n;
        let n = 1usize << log2n;
        let h = n / 2;
        let bins = h + 1;
        let budget = budget.max(1);
        match self.stage {
            Stage::Clear { t } => {
                let end = (t + budget).min(n);
                self.buf[t..end].iter_mut().for_each(|c| *c = Complex::ZERO);
                self.stage = if end == n {
                    Stage::Gather { layer: 0, t: 0 }
                } else {
                    Stage::Clear { t: end }
                };
                end - t
            }
            Stage::Gather { layer, t } => {
                let Some(l) = self.layers.get(layer) else {
                    self.stage = Stage::Forward(FftJobState(FftJob::new(log2n, false)));
                    return 1;
                };
                let (a, b) = overlap(l.frame_center, n, l.hit.len());
                let start = t.max(a);
                if l.frame_gain <= 0.0 || start >= b {
                    self.stage = Stage::Gather {
                        layer: layer + 1,
                        t: 0,
                    };
                    return 1;
                }
                let end = (start + budget).min(b);
                let w = &self.windows[(log2n - WINDOW_MIN_LOG2) as usize];
                let offset = l.frame_center - (n / 2) as i64;
                let g = l.frame_gain;
                let (src_l, src_r) = (&l.hit.left, &l.hit.right);
                for i in start..end {
                    let idx = (offset + i as i64) as usize;
                    let gw = g * w[i];
                    let c = &mut self.buf[i];
                    c.re += gw * src_l[idx];
                    c.im += gw * src_r[idx];
                }
                self.stage = if end >= b {
                    Stage::Gather {
                        layer: layer + 1,
                        t: 0,
                    }
                } else {
                    Stage::Gather { layer, t: end }
                };
                end - start
            }
            Stage::Forward(FftJobState(mut job)) => {
                let used = job.run(&self.tables, &mut self.buf, budget);
                self.stage = if job.done() {
                    Stage::Analyze { k: 0 }
                } else {
                    Stage::Forward(FftJobState(job))
                };
                used
            }
            Stage::Analyze { k } => {
                // Separate the two real spectra packed in one complex FFT.
                let end = (k + budget).min(bins);
                for k in k..end {
                    let z = self.buf[k];
                    let zc = self.buf[(n - k) & (n - 1)].conj();
                    self.mag[0][k] = 0.5 * (z + zc).abs();
                    self.mag[1][k] = 0.5 * (z - zc).abs();
                }
                self.stage = if end < bins {
                    Stage::Analyze { k: end }
                } else {
                    Stage::Transpose { k: 0 }
                };
                end - k
            }
            Stage::Transpose { k } => {
                if self.job.shift_semitones == 0.0 && self.job.sub <= 0.0 {
                    self.stage = Stage::SmearTable { k: 0 };
                    return 1;
                }
                let end = (k + budget / 2).max(k + 1).min(bins);
                self.transpose(k, end, bins);
                self.stage = if end < bins {
                    Stage::Transpose { k: end }
                } else {
                    std::mem::swap(&mut self.mag, &mut self.tmp);
                    Stage::SmearTable { k: 0 }
                };
                (end - k) * 2
            }
            Stage::SmearTable { k } => {
                let width = self.job.smear_octaves;
                let key = (log2n, width.to_bits());
                if width <= 1e-3 || self.smear_key == Some(key) {
                    self.stage = if width > 1e-3 {
                        Stage::Smear { pass: 0, k: 0 }
                    } else {
                        Stage::FocusEnv { k: 0 }
                    };
                    return 1;
                }
                let end = (k + budget / 4).max(k + 1).min(bins);
                for i in k..end {
                    self.smear_coeff[i] = blur_coeff(i, width);
                }
                self.stage = if end < bins {
                    Stage::SmearTable { k: end }
                } else {
                    self.smear_key = Some(key);
                    Stage::Smear { pass: 0, k: 0 }
                };
                (end - k) * 4
            }
            Stage::Smear { pass, k } => {
                let end = (k + budget).min(bins);
                let m = &mut self.mag[pass / 2][..bins];
                blur_pass(
                    m,
                    &self.smear_coeff,
                    pass % 2 == 0,
                    k,
                    end,
                    &mut self.blur_y,
                );
                self.stage = match (end < bins, pass) {
                    (true, _) => Stage::Smear { pass, k: end },
                    (false, 3) => Stage::FocusEnv { k: 0 },
                    (false, _) => Stage::Smear {
                        pass: pass + 1,
                        k: 0,
                    },
                };
                end - k
            }
            Stage::FocusEnv { k } => {
                if self.job.focus.abs() < 0.01 {
                    self.stage = Stage::GainTable { k: 0 };
                    return 1;
                }
                let end = (k + budget).min(bins);
                for i in k..end {
                    self.env[i] = 0.5 * (self.mag[0][i] + self.mag[1][i]);
                }
                self.stage = if end < bins {
                    Stage::FocusEnv { k: end }
                } else {
                    Stage::FocusBlur { pass: 0, k: 0 }
                };
                end - k
            }
            Stage::FocusBlur { pass, k } => {
                let end = (k + budget).min(bins);
                let coeff = &self.focus_coeff[(log2n - WINDOW_MIN_LOG2) as usize];
                blur_pass(
                    &mut self.env[..bins],
                    coeff,
                    pass == 0,
                    k,
                    end,
                    &mut self.blur_y,
                );
                self.stage = match (end < bins, pass) {
                    (true, _) => Stage::FocusBlur { pass, k: end },
                    (false, 0) => Stage::FocusBlur { pass: 1, k: 0 },
                    (false, _) => Stage::FocusApply { k: 0 },
                };
                end - k
            }
            Stage::FocusApply { k } => {
                let end = (k + budget / 4).max(k + 1).min(bins);
                let p = self.job.focus;
                for i in k..end {
                    let e = self.env[i];
                    if e < 1e-9 {
                        continue;
                    }
                    for ch in 0..2 {
                        let m = self.mag[ch][i];
                        self.mag[ch][i] = m * (m / e).max(1e-6).powf(p);
                    }
                }
                self.stage = if end < bins {
                    Stage::FocusApply { k: end }
                } else {
                    Stage::GainTable { k: 0 }
                };
                (end - k) * 4
            }
            Stage::GainTable { k } => {
                let s = self.job;
                let key = (log2n, s.tilt_db.to_bits(), s.lock.to_bits(), s.root);
                if self.gain_key == Some(key) {
                    self.stage = Stage::Colour { k: 0 };
                    return 1;
                }
                let end = (k + budget / 4).max(k + 1).min(bins);
                self.gain_table(n, k, end);
                self.stage = if end < bins {
                    Stage::GainTable { k: end }
                } else {
                    self.gain_key = Some(key);
                    Stage::Colour { k: 0 }
                };
                (end - k) * 4
            }
            Stage::Colour { k } => {
                if k == 0 {
                    self.frame_energy = 0.0;
                    self.frame_bands = [0.0; SPECTRUM_BANDS];
                    self.mag[0][0] = 0.0;
                    self.mag[1][0] = 0.0;
                }
                let end = (k + budget).min(bins);
                let bands = &self.band_of[(log2n - WINDOW_MIN_LOG2) as usize];
                for i in k.max(1)..end {
                    let g = self.gains[i];
                    let (l, r) = (self.mag[0][i] * g, self.mag[1][i] * g);
                    self.mag[0][i] = l;
                    self.mag[1][i] = r;
                    let p = l * l + r * r;
                    self.frame_energy += p;
                    if let Some(b) = self.frame_bands.get_mut(bands[i] as usize) {
                        *b = b.max(p);
                    }
                }
                self.stage = if end < bins {
                    Stage::Colour { k: end }
                } else {
                    Stage::Level
                };
                end - k
            }
            Stage::Level => {
                self.level(n);
                self.stage = Stage::Phases { k: 1 };
                SPECTRUM_BANDS
            }
            Stage::Phases { k } => {
                let end = (k + budget / 2).max(k + 1).min(h);
                self.phases(n, k, end);
                self.stage = if end < h {
                    Stage::Phases { k: end }
                } else {
                    Stage::Inverse(FftJobState(FftJob::new(log2n, true)))
                };
                (end - k) * 2
            }
            Stage::Inverse(FftJobState(mut job)) => {
                let used = job.run(&self.tables, &mut self.buf, budget);
                self.stage = if job.done() {
                    Stage::Synth { t: 0 }
                } else {
                    Stage::Inverse(FftJobState(job))
                };
                used
            }
            Stage::Synth { t } => {
                let end = (t + budget).min(n);
                let idx = (log2n - WINDOW_MIN_LOG2) as usize;
                let w = &self.windows[idx];
                let hinv = &self.hinv[idx];
                // After a window change the old tail is shorter or longer;
                // overlap what lines up.
                let tail_len = (1usize << self.tail_log2n) / 2;
                for i in t..end {
                    let y = if self.job_silent {
                        Complex::ZERO
                    } else {
                        self.buf[i]
                    };
                    let (yl, yr) = (w[i] * y.re, w[i] * y.im);
                    if i < h {
                        let (tl, tr) = if i < tail_len {
                            (self.tail[0][i], self.tail[1][i])
                        } else {
                            (0.0, 0.0)
                        };
                        self.next_out[0][i] = (yl + tl) * hinv[i];
                        self.next_out[1][i] = (yr + tr) * hinv[i];
                    } else {
                        self.next_tail[0][i - h] = yl;
                        self.next_tail[1][i - h] = yr;
                    }
                }
                self.stage = if end == n {
                    Stage::Done
                } else {
                    Stage::Synth { t: end }
                };
                end - t
            }
            Stage::Done => 0,
        }
    }

    /// SHIFT (spectral transposition) and SUB (the spectrum an octave down,
    /// blended in) for bins `k0..k1`, into `tmp`.
    fn transpose(&mut self, k0: usize, k1: usize, bins: usize) {
        let inv_ratio = (-self.job.shift_semitones / 12.0).exp2();
        let sub = self.job.sub;
        for ch in 0..2 {
            let m = &self.mag[ch][..bins];
            let read = |pos: f32| -> f32 {
                let i = pos as usize;
                if i + 1 >= bins {
                    return 0.0;
                }
                let f = pos - i as f32;
                m[i] + (m[i + 1] - m[i]) * f
            };
            for k in k0..k1 {
                let pos = k as f32 * inv_ratio;
                let mut v = read(pos);
                if sub > 0.0 {
                    v = (v * v + sub * sub * read(2.0 * pos).powi(2)).sqrt();
                }
                self.tmp[ch][k] = v;
            }
        }
    }

    /// TILT and harmonic LOCK gains for bins `k0..k1`, plus a rumble cut:
    /// FK-2 hits tuned near 20 Hz stretch into sub-audible energy that
    /// only eats headroom.
    fn gain_table(&mut self, n: usize, k0: usize, k1: usize) {
        let s = self.job;
        let bin_hz = self.sample_rate / n as f32;
        let tilt = s.tilt_db / 6.0206;
        let f0 = map::note_hz(s.root, 2);
        let sigma = (0.06f32).max(1.5 * bin_hz / f0);
        for k in k0..k1 {
            let f = k as f32 * bin_hz;
            let mut g = if tilt != 0.0 {
                (f.max(20.0) / map::TILT_PIVOT_HZ).powf(tilt)
            } else {
                1.0
            };
            if s.lock > 0.0 {
                let harm = f / f0;
                let d = if harm < 0.5 {
                    1.0
                } else {
                    (harm - harm.round()).abs()
                };
                g *= 1.0 - s.lock + s.lock * (-(d / sigma) * (d / sigma)).exp();
            }
            let rumble = ((f - RUMBLE_LO_HZ) / (RUMBLE_HI_HZ - RUMBLE_LO_HZ)).clamp(0.0, 1.0);
            self.gains[k] = g * rumble * rumble;
        }
    }

    /// The leveller and the panel's spectrum, from the finished frame.
    fn level(&mut self, n: usize) {
        let comp = self.compensation[(self.job_log2n - WINDOW_MIN_LOG2) as usize];
        // Per-sample output power (both channels averaged) before levelling.
        let power = self.frame_energy * comp * comp / (n as f32 * n as f32);
        let hop_seconds = (n / 2) as f32 / self.sample_rate;
        let tau = if power > self.agc_power {
            AGC_ATTACK_SECONDS
        } else {
            AGC_RELEASE_SECONDS
        };
        self.agc_power += (power - self.agc_power) * (1.0 - (-hop_seconds / tau).exp());
        self.agc_gain =
            (AGC_TARGET_RMS / self.agc_power.max(1e-20).sqrt()).clamp(AGC_MIN_GAIN, AGC_MAX_GAIN);
        // A full-scale sine peaks at about n/4 per bin; show 72 dB below that.
        let reference = comp * self.agc_gain / (n as f32 * 0.25);
        for (out, p) in self.spectrum.iter_mut().zip(self.frame_bands) {
            let db = 10.0 * (p.max(1e-30) * reference * reference).log10();
            let v = ((db + 72.0) / 72.0).clamp(0.0, 1.0);
            // Rise at once, fall slowly.
            *out = v.max(*out * 0.8);
        }
    }

    /// Random phases (correlated between the channels as WIDTH says) for
    /// bins `k0..k1`, packed back into one complex spectrum for the inverse
    /// FFT.
    fn phases(&mut self, n: usize, k0: usize, k1: usize) {
        let h = n / 2;
        if k0 <= 1 {
            self.buf[0] = Complex::ZERO;
            self.buf[h] = Complex::ZERO;
        }
        let comp = self.compensation[(self.job_log2n - WINDOW_MIN_LOG2) as usize];
        let scale = comp * self.agc_gain / n as f32;
        let width = (self.job.width.clamp(0.0, 1.0) * PHASES as f32) as u32;
        for k in k0.max(1)..k1 {
            let r = self.rng.next_u32();
            let il = (r >> 20) as usize & (PHASES - 1);
            let d = r & (PHASES as u32 - 1);
            let ir = (il + ((d * width) >> 12) as usize) & (PHASES - 1);
            let yl = self.phasors[il] * (self.mag[0][k] * scale);
            let yr = self.phasors[ir] * (self.mag[1][k] * scale);
            self.buf[k] = Complex::new(yl.re - yr.im, yl.im + yr.re);
            self.buf[n - k] = Complex::new(yl.re + yr.im, yr.re - yl.im);
        }
    }
}

/// Range of window positions `t` (in `0..n`) that fall inside a source of
/// `len` samples when the window is centred on `center`.
fn overlap(center: i64, n: usize, len: usize) -> (usize, usize) {
    let offset = center - (n / 2) as i64;
    let a = (-offset).clamp(0, n as i64) as usize;
    let b = (len as i64 - offset).clamp(0, n as i64) as usize;
    (a, b.max(a))
}

/// One-pole coefficient for smoothing across bins at bin `k` with a
/// bandwidth proportional to frequency (`octaves` wide).
fn blur_coeff(k: usize, octaves: f32) -> f32 {
    let width = (k as f32 * octaves * core::f32::consts::LN_2).max(1e-3);
    (-1.0 / width).exp()
}

/// Positions `j0..j1` of one pass of a zero-phase (forwards, then
/// backwards) bin smoother. `y` carries the filter state between slices.
fn blur_pass(m: &mut [f32], coeff: &[f32], forwards: bool, j0: usize, j1: usize, y: &mut f32) {
    let last = m.len() - 1;
    if j0 == 0 {
        *y = if forwards { m[0] } else { m[last] };
    }
    for j in j0..j1 {
        let k = if forwards { j } else { last - j };
        let a = coeff[k];
        *y = a * *y + (1.0 - a) * m[k];
        m[k] = *y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::OUTLINE;

    const SR: f32 = 48_000.0;

    fn hit(left: Vec<f32>) -> Rc<Hit> {
        Rc::new(Hit {
            right: left.clone(),
            left,
            algorithms: [0, 1],
            pitch_hz: [0.0; 2],
            outline: [(0.0, 0.0); OUTLINE],
            generation: 0,
        })
    }

    fn sine(hz: f32, seconds: f32, amp: f32) -> Vec<f32> {
        (0..(seconds * SR) as usize)
            .map(|i| amp * (core::f32::consts::TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn run(s: &mut Stretcher, settings: &StretchSettings, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let frames = (seconds * SR) as usize;
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        for (a, b) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            s.render(settings, a, b);
        }
        (l, r)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn settings(factor: f32) -> StretchSettings {
        StretchSettings {
            factor,
            log2n: 13,
            width: 1.0,
            freeze: false,
            shift_semitones: 0.0,
            sub: 0.0,
            smear_octaves: 0.0,
            focus: 0.0,
            lock: 0.0,
            tilt_db: 0.0,
            root: 9,
        }
    }

    /// Zero-crossing rate → frequency.
    fn pitch(x: &[f32]) -> f32 {
        let c = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        c as f32 / (x.len() as f32 / SR)
    }

    #[test]
    fn silent_without_layers() {
        let mut s = Stretcher::new(SR, 1);
        let (l, r) = run(&mut s, &settings(10.0), 1.0);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0));
    }

    #[test]
    fn stretches_a_tone_in_time_and_keeps_its_pitch() {
        let mut s = Stretcher::new(SR, 2);
        // 0.5 s of 440 Hz at 8x ≈ 4 s of sound.
        s.strike(hit(sine(440.0, 0.5, 0.25)));
        let st = settings(8.0);
        let (l, _) = run(&mut s, &st, 7.0);
        let sec = |a: f32, b: f32| &l[(a * SR) as usize..(b * SR) as usize];
        assert!(rms(sec(1.5, 3.0)) > 0.05, "middle {}", rms(sec(1.5, 3.0)));
        assert!(rms(sec(5.5, 7.0)) < rms(sec(1.5, 3.0)) * 0.05, "ended");
        let f = pitch(sec(1.5, 3.0));
        assert!((f / 440.0 - 1.0).abs() < 0.03, "{f}");
        assert_eq!(s.overruns, 0);
    }

    #[test]
    fn layers_mix_and_finish() {
        let mut s = Stretcher::new(SR, 3);
        let st = settings(4.0);
        s.strike(hit(sine(220.0, 0.3, 0.25)));
        run(&mut s, &st, 0.3);
        s.strike(hit(sine(330.0, 0.3, 0.25)));
        run(&mut s, &st, 0.5);
        assert_eq!(s.layer_count(), 2);
        run(&mut s, &st, 3.0);
        assert_eq!(s.layer_count(), 0);
    }

    #[test]
    fn freeze_holds_a_layer() {
        let mut s = Stretcher::new(SR, 4);
        let mut st = settings(2.0);
        s.strike(hit(sine(300.0, 0.3, 0.25)));
        run(&mut s, &st, 0.4);
        st.freeze = true;
        let (l, _) = run(&mut s, &st, 5.0);
        assert_eq!(s.layer_count(), 1);
        assert!(rms(&l[l.len() - 24_000..]) > 0.05);
    }

    #[test]
    fn transposition_moves_the_pitch() {
        let mut s = Stretcher::new(SR, 5);
        let mut st = settings(10.0);
        st.log2n = 14;
        st.shift_semitones = 12.0;
        s.strike(hit(sine(300.0, 1.0, 0.25)));
        let (l, _) = run(&mut s, &st, 4.0);
        let f = pitch(&l[(2.0 * SR) as usize..(4.0 * SR) as usize]);
        assert!((f / 600.0 - 1.0).abs() < 0.04, "{f}");
    }

    #[test]
    fn width_zero_is_mono() {
        let mut s = Stretcher::new(SR, 6);
        let mut st = settings(5.0);
        st.width = 0.0;
        s.strike(hit(sine(500.0, 0.5, 0.25)));
        let (l, r) = run(&mut s, &st, 2.0);
        let diff: f32 = l
            .iter()
            .zip(&r)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(diff < 1e-4, "{diff}");
        let mut s = Stretcher::new(SR, 6);
        st.width = 1.0;
        s.strike(hit(sine(500.0, 0.5, 0.25)));
        let (l, r) = run(&mut s, &st, 2.0);
        let corr: f32 = l.iter().zip(&r).map(|(a, b)| a * b).sum::<f32>()
            / (rms(&l) * rms(&r) * l.len() as f32);
        assert!(corr.abs() < 0.5, "{corr}");
    }

    #[test]
    fn many_layers_are_capped_and_levelled() {
        let mut s = Stretcher::new(SR, 7);
        let st = settings(200.0);
        for _ in 0..40 {
            s.strike(hit(sine(200.0, 0.5, 0.25)));
            run(&mut s, &st, 0.2);
        }
        let (l, r) = run(&mut s, &st, 4.0);
        let live = s.layers.iter().filter(|l| !l.evicted).count();
        assert!(live <= MAX_LAYERS, "{live}");
        assert!(s.layer_count() <= MAX_LAYERS + 16, "{}", s.layer_count());
        let level = rms(&l[48_000..]);
        assert!(level < 0.5 && level > 0.05, "{level}");
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    }

    #[test]
    fn window_changes_are_safe() {
        let mut s = Stretcher::new(SR, 8);
        let mut st = settings(20.0);
        s.strike(hit(sine(250.0, 1.0, 0.25)));
        for log2n in [11, 16, 12, 15, 13] {
            st.log2n = log2n;
            let (l, r) = run(&mut s, &st, 1.5);
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 2.0));
        }
    }

    #[test]
    fn largest_window_runs_within_budget() {
        let mut s = Stretcher::new(SR, 9);
        let mut st = settings(50.0);
        st.log2n = WINDOW_MAX_LOG2;
        st.smear_octaves = 0.2;
        st.focus = 1.0;
        st.lock = 0.5;
        st.tilt_db = -3.0;
        st.sub = 0.5;
        for _ in 0..8 {
            s.strike(hit(sine(110.0, 2.0, 0.25)));
        }
        run(&mut s, &st, 8.0);
        assert_eq!(s.overruns, 0);
    }
}
