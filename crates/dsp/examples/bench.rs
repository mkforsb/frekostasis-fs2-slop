//! Real-time factor of the whole machine per scene, while it is playing.
//!
//! ```sh
//! cargo run -p frekostasis-dsp --release --example bench
//! ```

use std::time::Instant;

use frekostasis_dsp::Machine;
use frekostasis_dsp::params::default_patch;
use frekostasis_dsp::presets::SCENES;

fn main() {
    let sr = 48_000.0;
    let seconds = 60.0;
    let mut worst = f64::MAX;
    for scene in SCENES {
        let mut m = Machine::new(sr, 99);
        m.load_patch(&scene.patch(&default_patch()));
        m.snap();
        m.set_playing(true);
        let mut l = [0.0f32; 128];
        let mut r = [0.0f32; 128];
        let blocks = (sr * seconds / 128.0) as usize;
        let mut slowest = 0.0f64;
        let start = Instant::now();
        for _ in 0..blocks {
            let t = Instant::now();
            m.render(&mut l, &mut r);
            slowest = slowest.max(t.elapsed().as_secs_f64());
        }
        let rtf = seconds as f64 / start.elapsed().as_secs_f64();
        worst = worst.min(rtf);
        println!(
            "{:<20} {rtf:>6.1}x real time, slowest block {:.2} ms of {:.2} ms",
            scene.name,
            slowest * 1e3,
            128.0 / sr * 1e3
        );
    }
    println!("worst case: {worst:.1}x");
}
