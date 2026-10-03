//! When to strike: a Euclidean pattern clocked in seconds, and the planner
//! that decides how many hits to keep rendered ahead of it.

use crate::params::{Param, Patch, map};
use crate::source::POOL_MAX;

/// `pulses` spread as evenly as possible over `steps`, rotated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub steps: u32,
    pub pulses: u32,
    pub rotate: u32,
}

impl Pattern {
    pub fn from_patch(p: &Patch) -> Self {
        let v = |param: Param| param.sanitize(p[param.index()]) as u32;
        let steps = v(Param::Steps).max(1);
        Self {
            steps,
            pulses: v(Param::Pulses).clamp(1, steps),
            rotate: v(Param::Rotate) % steps,
        }
    }

    /// Bresenham's form of Bjorklund's algorithm: E(3, 8) = x..x..x.
    pub fn is_pulse(&self, step: u32) -> bool {
        let n = self.steps.max(1);
        let k = self.pulses.min(n);
        let i = (step + self.rotate) % n;
        (i * k) % n < k
    }

    /// Bit `i` set when step `i` is a pulse.
    pub fn mask(&self) -> u32 {
        (0..self.steps).fold(0, |m, i| m | ((self.is_pulse(i) as u32) << i))
    }
}

/// Step clock driven by the output sample count.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    /// Position within the current step, 0..1.
    phase: f64,
    step: u32,
}

impl Clock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn step(&self) -> u32 {
        self.step
    }

    pub fn phase(&self) -> f32 {
        self.phase as f32
    }

    /// Advance by `frames`. Returns the new step if one began. Steps are at
    /// least 0.25 s, far longer than a block, so at most one begins per call.
    pub fn advance(
        &mut self,
        frames: usize,
        step_seconds: f32,
        sample_rate: f32,
        steps: u32,
    ) -> Option<u32> {
        self.phase += frames as f64 / (step_seconds as f64 * sample_rate as f64);
        if self.phase < 1.0 {
            return None;
        }
        self.phase = (self.phase - 1.0).min(0.999);
        self.step = (self.step + 1) % steps.max(1);
        Some(self.step)
    }
}

/// Most hits kept ready.
pub const POOL_TARGET_MAX: usize = 6;

/// How many hits to keep rendered so the pattern never waits for one.
///
/// Rendering a hit takes about `render_seconds`. Over any stretch of the
/// pattern holding `m` pulses and lasting `t`, `m` hits are needed but only
/// `⌊t / render_seconds⌋` can be made meanwhile: the pool must cover the
/// worst such shortfall (checked over two cycles, so bursts that wrap
/// around count), plus one spare.
pub fn pool_target(pattern: &Pattern, step_seconds: f32, render_seconds: f32) -> usize {
    let times: Vec<f32> = (0..2 * pattern.steps)
        .filter(|&i| pattern.is_pulse(i % pattern.steps))
        .map(|i| i as f32 * step_seconds)
        .collect();
    let render = render_seconds.max(1e-3);
    let mut need = 1usize;
    for i in 0..times.len() {
        for j in i..times.len() {
            let pulses = j - i + 1;
            let made = ((times[j] - times[i]) / render) as usize;
            need = need.max(pulses.saturating_sub(made));
        }
    }
    (need + 1).clamp(1, POOL_TARGET_MAX.min(POOL_MAX))
}

/// Pattern, step length and planner inputs straight from a patch.
pub fn pool_target_for(patch: &Patch, render_seconds: f32) -> usize {
    pool_target(
        &Pattern::from_patch(patch),
        map::step_seconds(patch[Param::StepTime.index()]),
        render_seconds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(steps: u32, pulses: u32, rotate: u32) -> Pattern {
        Pattern {
            steps,
            pulses,
            rotate,
        }
    }

    fn render(p: &Pattern) -> String {
        (0..p.steps)
            .map(|i| if p.is_pulse(i) { 'x' } else { '.' })
            .collect()
    }

    #[test]
    fn euclidean_patterns() {
        assert_eq!(render(&pat(8, 3, 0)), "x..x..x.");
        assert_eq!(render(&pat(8, 3, 1)), "..x..x.x");
        assert_eq!(render(&pat(4, 4, 0)), "xxxx");
        assert_eq!(render(&pat(5, 2, 0)), "x..x.");
        assert_eq!(render(&pat(1, 1, 0)), "x");
        assert_eq!(pat(8, 3, 0).mask(), 0b0100_1001);
        for n in 1..=16 {
            for k in 1..=n {
                assert_eq!(pat(n, k, 0).mask().count_ones(), k);
            }
        }
    }

    #[test]
    fn clock_steps_on_time() {
        let mut c = Clock::default();
        let mut starts = vec![];
        for block in 0..1000 {
            if let Some(s) = c.advance(100, 0.5, 1000.0, 4) {
                starts.push((block, s));
            }
        }
        // 500 frames per step = every 5 blocks.
        assert_eq!(starts[0], (4, 1));
        assert_eq!(starts[1], (9, 2));
        assert_eq!(starts[3], (19, 0));
        assert_eq!(starts.len(), 200);
    }

    #[test]
    fn pool_grows_with_density_and_render_time() {
        let sparse = pool_target(&pat(8, 2, 0), 4.0, 0.5);
        let dense = pool_target(&pat(8, 8, 0), 0.25, 0.5);
        let burst = pool_target(&pat(16, 5, 0), 0.25, 0.5);
        assert_eq!(sparse, 2);
        assert!(dense > sparse, "{dense}");
        assert!(dense <= POOL_TARGET_MAX);
        assert!(burst >= 2 && burst <= dense);
        // Instant rendering: one spare is enough.
        assert_eq!(pool_target(&pat(16, 16, 0), 1.0, 1e-3), 2);
    }
}
