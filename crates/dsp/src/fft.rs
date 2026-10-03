//! Radix-2 complex FFT that can be run a slice at a time.
//!
//! The stretcher needs one forward and one inverse FFT of up to 65 536
//! points per output hop. Done in one go, that is several milliseconds: more
//! than a whole 128-frame AudioWorklet block. An [`FftJob`] instead does a
//! bounded amount of work per call and picks up where it left off, so the
//! transform can be spread evenly over the hundreds of blocks a hop lasts.

use core::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    #[inline]
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    #[inline]
    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    #[inline]
    pub fn norm_sqr(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    #[inline]
    pub fn abs(self) -> f32 {
        self.norm_sqr().sqrt()
    }
}

impl core::ops::Add for Complex {
    type Output = Complex;
    #[inline]
    fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }
}

impl core::ops::Sub for Complex {
    type Output = Complex;
    #[inline]
    fn sub(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }
}

impl core::ops::Mul for Complex {
    type Output = Complex;
    #[inline]
    fn mul(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl core::ops::Mul<f32> for Complex {
    type Output = Complex;
    #[inline]
    fn mul(self, k: f32) -> Complex {
        Complex::new(self.re * k, self.im * k)
    }
}

/// Twiddle factors for every power-of-two size up to `2^max_log2`.
#[derive(Clone, Debug)]
pub struct FftTables {
    max_log2: u32,
    /// `exp(-2πi·k/N_max)` for `k < N_max/2`.
    twiddles: Vec<Complex>,
}

impl FftTables {
    pub fn new(max_log2: u32) -> Self {
        let n = 1usize << max_log2;
        let twiddles = (0..n / 2)
            .map(|k| {
                // f64 keeps the large tables accurate.
                let a = -(TAU as f64) * k as f64 / n as f64;
                Complex::new(a.cos() as f32, a.sin() as f32)
            })
            .collect();
        Self { max_log2, twiddles }
    }

    pub fn max_log2(&self) -> u32 {
        self.max_log2
    }
}

/// An in-progress transform of `2^log2n` points. Unnormalized in both
/// directions: `inverse(forward(x)) = n·x`.
#[derive(Clone, Copy, Debug)]
pub struct FftJob {
    log2n: u32,
    inverse: bool,
    /// 0 = bit-reversal permutation, `1..=log2n` = butterfly passes.
    stage: u32,
    /// Progress through the current stage.
    pos: usize,
}

impl FftJob {
    pub fn new(log2n: u32, inverse: bool) -> Self {
        Self {
            log2n,
            inverse,
            stage: 0,
            pos: 0,
        }
    }

    /// Total work units of a transform: one per element of the permutation,
    /// one per butterfly.
    pub fn cost(log2n: u32) -> usize {
        let n = 1usize << log2n;
        n + n / 2 * log2n as usize
    }

    pub fn done(&self) -> bool {
        self.stage > self.log2n
    }

    /// Do up to `budget` units of work on `data` (which must hold at least
    /// `2^log2n` points and not be touched by anything else until the job is
    /// [`done`](Self::done)). Returns the units used.
    pub fn run(&mut self, tables: &FftTables, data: &mut [Complex], budget: usize) -> usize {
        debug_assert!(self.log2n <= tables.max_log2);
        let n = 1usize << self.log2n;
        let data = &mut data[..n];
        let mut used = 0;
        while used < budget && !self.done() {
            if self.stage == 0 {
                let end = (self.pos + (budget - used)).min(n);
                let shift = usize::BITS - self.log2n;
                for i in self.pos..end {
                    let j = if self.log2n == 0 {
                        0
                    } else {
                        i.reverse_bits() >> shift
                    };
                    if j > i {
                        data.swap(i, j);
                    }
                }
                used += end - self.pos;
                self.pos = end;
                if self.pos == n {
                    self.stage = 1;
                    self.pos = 0;
                }
            } else {
                let s = self.stage;
                let half = 1usize << (s - 1);
                let stride = 1usize << (tables.max_log2 - s);
                let end = (self.pos + (budget - used)).min(n / 2);
                for b in self.pos..end {
                    let j = b & (half - 1);
                    let i = ((b >> (s - 1)) << s) | j;
                    let k = i + half;
                    let mut w = tables.twiddles[j * stride];
                    if self.inverse {
                        w = w.conj();
                    }
                    let t = data[k] * w;
                    let u = data[i];
                    data[i] = u + t;
                    data[k] = u - t;
                }
                used += end - self.pos;
                self.pos = end;
                if self.pos == n / 2 {
                    self.stage += 1;
                    self.pos = 0;
                }
            }
        }
        used
    }
}

/// Whole transform in one call.
pub fn fft(tables: &FftTables, data: &mut [Complex], inverse: bool) {
    let log2n = data.len().trailing_zeros();
    debug_assert!(data.len().is_power_of_two());
    FftJob::new(log2n, inverse).run(tables, data, usize::MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dft(x: &[Complex]) -> Vec<Complex> {
        let n = x.len();
        (0..n)
            .map(|k| {
                x.iter().enumerate().fold(Complex::ZERO, |acc, (t, &v)| {
                    let a = -TAU * ((k * t) % n) as f32 / n as f32;
                    acc + v * Complex::new(a.cos(), a.sin())
                })
            })
            .collect()
    }

    fn signal(n: usize) -> Vec<Complex> {
        let mut rng = crate::util::Noise::new(3);
        (0..n)
            .map(|_| Complex::new(rng.sample(), rng.sample()))
            .collect()
    }

    fn max_err(a: &[Complex], b: &[Complex]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (*x - *y).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn matches_the_dft() {
        let tables = FftTables::new(10);
        for log2n in [0, 1, 2, 5, 8] {
            let x = signal(1 << log2n);
            let mut y = x.clone();
            fft(&tables, &mut y, false);
            assert!(max_err(&y, &dft(&x)) < 1e-3, "n = {}", 1 << log2n);
        }
    }

    #[test]
    fn inverse_undoes_forward() {
        let tables = FftTables::new(16);
        let x = signal(1 << 16);
        let mut y = x.clone();
        fft(&tables, &mut y, false);
        fft(&tables, &mut y, true);
        let n = y.len() as f32;
        let back: Vec<Complex> = y.iter().map(|v| *v * (1.0 / n)).collect();
        assert!(max_err(&back, &x) < 1e-4);
    }

    #[test]
    fn sliced_runs_equal_one_shot() {
        let tables = FftTables::new(14);
        let x = signal(1 << 12);
        let mut whole = x.clone();
        fft(&tables, &mut whole, false);
        let mut sliced = x.clone();
        let mut job = FftJob::new(12, false);
        let mut total = 0;
        let mut calls = 0;
        while !job.done() {
            total += job.run(&tables, &mut sliced, 997);
            calls += 1;
        }
        assert_eq!(whole, sliced);
        assert_eq!(total, FftJob::cost(12));
        assert_eq!(calls, FftJob::cost(12).div_ceil(997));
    }

    #[test]
    fn pure_tone_lands_in_its_bin() {
        let tables = FftTables::new(10);
        let n = 1024;
        let mut x: Vec<Complex> = (0..n)
            .map(|t| Complex::new((TAU * 37.0 * t as f32 / n as f32).cos(), 0.0))
            .collect();
        fft(&tables, &mut x, false);
        assert!((x[37].abs() - n as f32 / 2.0).abs() < 0.1);
        assert!(x[36].abs() < 0.01 && x[38].abs() < 0.01);
    }
}
