//! Render every factory scene to a WAV file: press PLAY, let it drone, press
//! STOP and let the tails ring out.
//!
//! ```sh
//! cargo run -p frekostasis-dsp --release --example render -- out_dir [seconds]
//! ```

use std::io::Write;

use frekostasis_dsp::Machine;
use frekostasis_dsp::params::default_patch;
use frekostasis_dsp::presets::SCENES;
use frekostasis_dsp::telemetry as t;

const SR: u32 = 48_000;

fn write_wav(path: &std::path::Path, left: &[f32], right: &[f32]) -> std::io::Result<()> {
    let frames = left.len() as u32;
    let data_len = frames * 4;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&SR.to_le_bytes())?;
    f.write_all(&(SR * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for (l, r) in left.iter().zip(right) {
        for s in [l, r] {
            f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    f.flush()
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "renders".into()));
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60.0);
    std::fs::create_dir_all(&dir)?;
    for (i, scene) in SCENES.iter().enumerate() {
        let mut m = Machine::new(SR as f32, 1234 + i as u32);
        m.load_patch(&scene.patch(&default_patch()));
        m.snap();
        m.set_playing(true);
        let play = (seconds * SR as f32) as usize;
        let tail = 10 * SR as usize;
        let mut left = vec![0.0; play + tail];
        let mut right = vec![0.0; play + tail];
        let mut tel = [0.0; t::LEN];
        let (mut max_layers, mut max_pool) = (0.0f32, 0.0f32);
        for (n, (l, r)) in left.chunks_mut(128).zip(right.chunks_mut(128)).enumerate() {
            if n * 128 >= play && m.is_playing() {
                m.set_playing(false);
            }
            m.render(l, r);
            if n % 64 == 0 {
                m.telemetry(&mut tel);
                max_layers = max_layers.max(tel[t::LAYERS]);
                max_pool = max_pool.max(tel[t::POOL_TARGET]);
            }
        }
        m.telemetry(&mut tel);
        let body = &left[(5 * SR) as usize..play];
        let peak = left
            .iter()
            .chain(&right)
            .fold(0.0f32, |a, s| a.max(s.abs()));
        let rms = (body.iter().map(|s| s * s).sum::<f32>() / body.len() as f32).sqrt();
        let name: String = scene
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, &left, &right)?;
        println!(
            "{:<20} peak {:>5.3} rms {:>5.3} strikes {:>3} starved {:>2} layers≤{:>2} pool≤{} overruns {} -> {}",
            scene.name,
            peak,
            rms,
            tel[t::STRIKES],
            tel[t::STARVED],
            max_layers,
            max_pool,
            tel[t::OVERRUNS],
            path.display()
        );
    }
    Ok(())
}
