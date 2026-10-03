//! Spectral summary of rendered WAVs (16-bit stereo), over a stretch of time:
//! centroid, flatness (0 = pure tones, 1 = white noise), stereo correlation
//! and the strongest peaks.
//!
//! ```sh
//! cargo run -p frekostasis-dsp --release --example analyze -- from_s to_s file.wav…
//! ```

use frekostasis_dsp::fft::{Complex, FftTables, fft};

fn read_wav(path: &str) -> (u32, Vec<f32>, Vec<f32>) {
    let bytes = std::fs::read(path).expect("read");
    let sr = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let data = &bytes[44..];
    let (mut l, mut r) = (vec![], vec![]);
    for f in data.as_chunks::<4>().0 {
        l.push(i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0);
        r.push(i16::from_le_bytes([f[2], f[3]]) as f32 / 32768.0);
    }
    (sr, l, r)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let from: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let to: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30.0);
    for path in args {
        let (sr, l, r) = read_wav(&path);
        let (a, b) = (
            (from * sr as f32) as usize,
            ((to * sr as f32) as usize).min(l.len()),
        );
        let (l, r) = (&l[a..b], &r[a..b]);
        let corr = l.iter().zip(r).map(|(x, y)| x * y).sum::<f32>()
            / (l.iter().map(|x| x * x).sum::<f32>() * r.iter().map(|x| x * x).sum::<f32>()).sqrt();
        let log2n = (l.len() as f32).log2().ceil() as u32;
        let n = 1usize << log2n;
        let tables = FftTables::new(log2n);
        let mut buf = vec![Complex::ZERO; n];
        for (i, x) in l.iter().enumerate() {
            let w = 0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / l.len() as f32).cos();
            buf[i] = Complex::new(x * w, 0.0);
        }
        fft(&tables, &mut buf, false);
        let bin_hz = sr as f32 / n as f32;
        let power: Vec<f32> = buf[..n / 2].iter().map(|c| c.norm_sqr()).collect();
        let total: f32 = power.iter().sum();
        let centroid = power
            .iter()
            .enumerate()
            .map(|(k, p)| k as f32 * bin_hz * p)
            .sum::<f32>()
            / total;
        let band: Vec<f64> = power
            .iter()
            .enumerate()
            .filter(|(k, _)| (50.0..8000.0).contains(&(*k as f32 * bin_hz)))
            .map(|(_, p)| *p as f64 + 1e-20)
            .collect();
        let geo = (band.iter().map(|p| p.ln()).sum::<f64>() / band.len() as f64).exp();
        let flatness = geo / (band.iter().sum::<f64>() / band.len() as f64);
        // Strongest peaks at least 20 Hz apart.
        let mut idx: Vec<usize> = (1..n / 2 - 1)
            .filter(|&k| power[k] > power[k - 1] && power[k] >= power[k + 1])
            .collect();
        idx.sort_by(|&x, &y| power[y].partial_cmp(&power[x]).unwrap());
        let mut peaks: Vec<f32> = vec![];
        for k in idx {
            let f = k as f32 * bin_hz;
            if peaks.iter().all(|p| (p - f).abs() > 20.0) {
                peaks.push(f);
            }
            if peaks.len() == 6 {
                break;
            }
        }
        let name = std::path::Path::new(&path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        println!(
            "{name:<20} centroid {centroid:>6.0} Hz  flatness {flatness:.3}  L/R corr {corr:+.2}  peaks {:?}",
            peaks.iter().map(|p| p.round() as i32).collect::<Vec<_>>()
        );
    }
}
