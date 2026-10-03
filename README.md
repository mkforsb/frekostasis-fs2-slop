# Frekostasis FS-2

An experimental FM drone machine, the sibling of
[Xenostasis XS-1](../xenostasis), and built the same way. It is a
**Frekussion FK-2** with the panel ripped off: the dual four-operator FM
percussion synthesizer with both MUTATE knobs turned to max, and a clock
that keeps pressing Space. Every strike is *rendered instead of played*.
Its signal level shows when the sound has died away, and it is then fed to
a PaulStretch-style spectral time stretcher that smears it 1× to 1000×
longer. Dozens of these stretched FM strikes (clangs, bells, growls and
cross-modulated chaos) overlap into a slowly shifting drone, which then goes
through a ping-pong space echo and a Dattorro plate with shimmer.

*Freq* + *stasis*: frequency modulation with time taken out. A 0.7-second
FM clang becomes a minute of shimmering inharmonic haze.

It's written in Rust with a [Dioxus](https://dioxuslabs.com) UI and builds
for two targets from one codebase:

| Target | Audio | How |
| --- | --- | --- |
| Web (wasm) | WebAudio `AudioWorklet` | `make serve-web` |
| Native Linux | PulseAudio (also PipeWire's pulse server) | `make run-desktop` |

## Building

Requirements:

- Rust (stable) with `rustup target add wasm32-unknown-unknown`
- Dioxus CLI 0.7: `cargo binstall dioxus-cli` or grab `dx` from the Dioxus releases
- Desktop only (Debian/Ubuntu package names): `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libpulse-dev pkg-config`

```sh
make serve-web      # http://127.0.0.1:8080, press PLAY
make run-desktop    # native window
make web desktop    # release bundles in target/dx/frekostasis/release/
make test lint
make renders        # every scene: 60 s of drone + 10 s of tails → ./renders/*.wav
make analyze        # spectral summary of the renders
make bench          # whole-machine real-time factor and slowest block
```

For the desktop build, `FREKOSTASIS_LATENCY_MS` (default 60) sets the
PulseAudio target latency. A drone has no need for low latency, so both
targets buffer generously.

## Playing it

- **PLAY / STOP** (`Space`). PLAY strikes at once and starts the pattern.
  STOP fades the drone out over a couple of seconds while the echo and the
  reverb ring on.
- **STRIKE** (`X`) strikes a hit now, outside the pattern.
- **FREEZE** (`F`) stops time. Every layer holds its position and keeps
  being re-synthesized with fresh random phases: an endless, living chord.
  No new strikes arrive (except manual ones).
- **Scenes** (10) are in the header: Bell Cathedral, Gamelan Drift, Organ of
  Iron, Cross Currents, Sine Choir, Dub Chamber, Chaos Pair, Deep Carrier,
  Frozen Glass and Init. The whole panel is saved automatically:
  to `localStorage` on the web, and to `~/.config/frekostasis/state.txt` on the
  desktop.
- Knobs work as on the FK-2: drag vertically, `Shift` for fine control, scroll
  to nudge (stepped controls move one notch), double-click to reset.

### PATTERN: when to strike

A Euclidean pattern: **PULSES** spread as evenly as possible over **STEPS**,
shifted by **ROTATE**. E(3, 8) is the tresillo, `x..x..x.`. **STEP** is the
length of one step, from 0.25 s to 30 s. **CHANCE** lets a pulse skip now
and then. The ring shows the pattern as a necklace, with a hand sweeping
through the current step. It ripples on every strike.

### SOURCE: the headless FK-2

| Control | What it does |
| --- | --- |
| **ALGORITHMS** | Which of the FK-2's eight routings MUTATE may pick, drawn as on the FK-2 (carriers teal, modulators pink). Click to toggle, double-click to solo. Stacks give deep, noisy, metallic FM; pairs and fans give clusters; the additive ones give bells and organ-like tone. The algorithm can't be interpolated, so at partial MUTATE it jumps with probability MUTATE, and always leaves a disabled one. |
| **MUTATE** | Exactly the FK-2's knob, on both channels at every strike: `current + (random − current) × MUTATE`, with the FK-2's own RND ranges (every carrier audible, noise, drive, sweep and cross modulation often left out). At 100% (default) every strike is a brand new pair of sounds. Lower, it becomes a random walk. |
| **ORIGIN** | The FK-2 kit the walk starts from (and restarts from when changed). Kits set both channels, often with cross modulation between them. At MUTATE 0 every strike is this kit. |
| **PITCH**, **RANGE** | Each channel's TUNE is drawn from PITCH ± RANGE octaves. |
| **SCALE**, **ROOT** | …and snapped to a scale: FREE, OCT, 5TH, PENTA, MINOR, DORIAN, LYDIAN, WHOLE. When snapping, an LFO on pitch is tamed so strikes stay in tune. |
| **LENGTH** | The FK-2's amp DECAY (≈ 130 ms … 10 s of ring), with a little spread. Carrier envelopes multiply the amp envelope, so carriers are made to ring at least as long. |
| **RATIOS** | The operator ratios allowed: **ALL**, **HARM** (multiples of ¼: organ, pad, vowel) or **INHARM** (the FK-2's membrane, bar and "clang" ratios √2, π, 5.40…: bells, gongs, metal). Every operator snaps to the nearest allowed ratio. |
| **INDEX** | Scales every modulator's level: FM brightness from pure sines (×0) through the FK-2's own (×1) to four times the modulation index. |
| **CROSS** | Scales the XMOD and RING between the two channels, from independent channels (off) to four times what MUTATE rolled. |
| **GRIT** | Upper limit on the FK-2's DRIVE and N>FM, and on operator 4 FEEDBACK (clean saw-like tones at 0, the FK-2's deliberate noise at 1). |

LINK is always off: both channels always strike together anyway. Changing
any of these makes the already rendered hits out of date. They stay in the
pool (better than silence) until fresh ones replace them.

### STRETCH and SPECTRAL

| Control | What it does |
| --- | --- |
| **STRETCH** | 1× … 1000×, logarithmic (default 50.0×). The panel shows what it does to a typical hit, e.g. `1.19 s ⟶ 59.5 s`. |
| **WINDOW** | FFT size, 2K … 64K (43 ms … 1.4 s at 48 kHz). Small windows keep some motion and grain, large ones melt everything into pure, still tone. |
| **WIDTH** | Stereo phase decorrelation. At 0 both channels share the random phases (mono), at 100% they are independent. |
| **SHIFT** | Spectral transposition, ±24 semitones. |
| **SUB** | Blends in the spectrum an octave down. |
| **SMEAR** | Blurs the spectrum across frequency, up to half an octave: tones spread into bands. |
| **FOCUS** | Bipolar. Left flattens each region of the spectrum towards its envelope (breath, noise). Right sharpens the peaks against it (glassy, crystalline tones). |
| **LOCK** | Filters the spectrum towards the harmonic series of ROOT. Chaotic material starts to sing in key. |
| **TILT** | ±6 dB/octave around 600 Hz. |

### SPACE ECHO

A ping-pong delay. The input enters the left line, every left repeat feeds
the right line, and every right repeat feeds back to the left through
FEEDBK (up to 110%, soft-clipped, so it can run away like a pushed tape
echo).

- **TIME**: 20 ms … 2.5 s, or with **SYNC** a division of the pattern's step
  (1/16 … 1), so the echoes breathe with the strikes.
- **SPACING**: the right side's time relative to the left: EVEN (1:1), DOTTED
  (¾), TRIPLET (⅔), **GOLDEN** (1/φ) and **SILVER** (1/√2). The irrational
  ones never line up again, so the ping-pong spreads into an irregular,
  drifting cloud instead of a rhythm.
- **MODE**: what happens to every repeat. The TONE knob is relabelled for each:
  - **TAPE** (TONE = cutoff): lowpass, a gentle highpass, tape saturation,
    and wow and flutter that differ between the two heads.
  - **BAND** (CENTER): a band-pass on every pass, so the repeats thin into a
    ringing, radio-like band.
  - **SPIRAL** (SHIFT, ±30 Hz): a single-sideband frequency shifter in the
    loop, built from a Hilbert-transform allpass pair. Each repeat is shifted
    again, climbing or falling in an endless barber-pole spiral. Shifting by
    Hz rather than by a ratio also pulls harmonics out of tune with each
    other, which sounds suitably alien.
  - **DIFFUSE** (BLUR): three allpasses in the loop smear each repeat a
    little more, until the echoes dissolve into clouds.
- Delay times glide like a tape motor when changed.

### SHIMMER PLATE

The FK-2's Dattorro plate (the family plate), 1.4× larger, with DECAY up to 60 s. **SHIMMER**
puts a pitch shifter in each half of the tank; **INTERVAL** picks +OCT,
+5TH, +OCT+5, +2OCT, −OCT, or ±OCT (left half up, right half down).

The classic shimmer feeds a pitch-shifted copy of the reverb's output back
into its input. That loop's gain is the reverb's gain for a steady signal,
which is far above one for a 60-second tail, so it has to be limited to
survive. Here each shifter sits *inside* the tank, between the first delay
and the damping filter. On every trip around the figure-of-eight, a SHIMMER
fraction of the signal is replaced by its shifted copy. The shifter's two
grains are crossfaded with `sin² + cos² = 1`, so it never adds energy. The
loop gain therefore stays the tank's decay gain, which is below one, however
high DECAY and SHIMMER go. Each pass climbs another interval, and the
damping filter eats the top.

Both effects switch **completely off**. They fade out, stop processing and
clear their buffers.

### STRATA

The middle of the panel shows what the machine is doing, from telemetry the
audio engine reports about thirty times a second:

- **SPECTRUM**: the drone's spectrum, 30 Hz … 16 kHz.
- **STRATA**: one band per stretched layer, coloured by its two FK-2 algorithms,
  showing how far the stretcher has read through it. A bright head marks the
  read position, and faint layers are fading in or out.
- **POOL**: rendered hits waiting to be struck (bright = fresh, dim = out of
  date), the planned depth, and progress of the strike being rendered.
- **LAST STRIKE**: its waveform, the two routings, pitches and length.

## How it works

```
 CLOCK (Euclid) ──strike──┐
                          ▼
 FK-2 (MUTATE, SCALE…) ─render→ POOL ─→ STRETCHER ──→ ECHO ──→ SHIMMER PLATE ──→ MASTER
   a slice per block    ▲       layers summed, one FFT per hop
                        └── planner: how many hits to keep ready
```

Everything runs on the audio thread, in bounded slices per 128-frame block.
The web and desktop builds run exactly the same code.

**Rendering strikes.** A strike is a fresh FK-2 `Engine` with both channels
mutated, shaped and triggered. It renders at 4× real time (6× when the pool
is empty) into a scratch buffer, a slice per block. Every 512 frames the
renderer looks at the level: once a window falls 50 dB below the loudest
window (or below −100 dBFS), the sound is over. It is then trimmed, faded,
normalized so its loudest ~85 ms has the same RMS as every other hit, and
copied out, again a slice at a time. Silent rolls are thrown away. Anything
still ringing at 8 s is faded out.

**How many hits to keep ready.** Rendering a hit takes about
`average hit length / 4` of real time. Over any stretch of the pattern that
holds `m` pulses and lasts `t` seconds, `m` hits are needed, but only
`⌊t / render time⌋` can be made in the meantime. The planner checks every
such stretch over two cycles (so bursts that wrap around count) and keeps the
worst shortfall plus one spare (at most 6). It re-plans whenever the pattern,
the step or the measured hit length changes. If the pool still runs dry, the
last hit is struck again (the panel counts these as "re-used"). A strike
that comes before anything has been rendered at all is *owed*, and fires as
soon as the first hit is ready.

**The stretcher.** It follows Nasca Octavian Paul's PaulStretch: take a
long window of the input (window `(1 − x²)^1.25`), keep only the spectrum's
magnitudes, give every bin a random phase, and overlap-add at half a
window. The input advances `hop / STRETCH` per hop. Random-phase frames are
uncorrelated, so the overlap is corrected for their *power*,
`1/√(w(t)² + w(t + N/2)²)`, not their amplitude.

- **All layers share one transform.** Windowing and the FFT are linear, and
  the phases are thrown away anyway. So each active layer's window (each
  layer at its own read position) is **summed before the FFT**. A frame costs
  one forward and one inverse FFT, however many of the up-to-24 layers are
  sounding.
- **Stereo shares one transform too.** Left goes in the real part and right
  in the imaginary part of a single complex FFT. The two spectra are
  separated using `X_L[k] = (Z[k] + Z*[N−k]) / 2`, and packed back around the
  random phases before the inverse FFT.
- **The work is spread out.** A 64K-point FFT takes about a millisecond
  natively, which would blow through a 2.7 ms AudioWorklet block along with
  everything else. So the frame for the *next* hop is built as a pipeline of
  resumable stages while the current hop plays: gather, forward FFT (a radix-2
  FFT that can stop after any butterfly), separate, transpose, smear, focus,
  colour, level, phases, inverse FFT, overlap-add. Each block grants the
  pipeline a work budget sized to finish the frame at 70% of the hop. The
  per-bin coefficient tables (smear, tilt/lock gains, spectrum bands) are
  cached and only rebuilt, again in slices, when their knobs move.
- **Levelling.** A slow leveller (1 s attack, 6 s release, −30 … +12 dB of
  range) keeps the drone steady whether one layer or twenty are sounding.
  A rumble cut removes the sub-audible energy that 20 Hz strikes stretch
  into.

`make bench` plays every scene for a minute. It runs 30–77× real time on
one desktop core. The FK-2 is a little heavier to render than the XK-1, and
the slowest single block takes about 0.6–1.2 ms of its 2.67 ms (it varies
from run to run with the machine's load).

## Layout

```
crates/fk2        the Frekussion FK-2 DSP, vendored unchanged (operators, algorithms, kits, plate…)
crates/dsp        the drone machine; no dependencies besides fk2; 54 unit tests
  src/source.rs     the FK-2's RND/MUTATE (algorithm-mask aware), SOURCE shaping, level-watching
                    hit renderer, pool
  src/util.rs       small DSP building blocks (noise, delay line, SVF, DC blocker), from the XK-1
  src/clock.rs      Euclidean patterns, the step clock, the pool planner
  src/fft.rs        resumable radix-2 FFT
  src/stretch.rs    the layered PaulStretch pipeline
  src/echo.rs       ping-pong space echo, Hilbert frequency shifter
  src/plate.rs      Dattorro plate with in-tank shimmer
  src/machine.rs    everything wired together, transport, telemetry
  examples/         render (scenes → WAV), analyze (spectral summary), bench
crates/worklet    C ABI over the machine, compiled to a standalone wasm module with no imports
crates/app        Dioxus UI with `web` and `desktop` features
  build.rs          (web) builds crates/worklet for wasm32 and embeds it
  src/audio/        web.rs + worklet.js (AudioWorklet), pulse.rs (PulseAudio thread)
```

The UI never touches the machine directly. It sends small commands (set a
parameter, play/stop, strike): over the worklet's `MessagePort` on the web,
and over an `mpsc` channel on the desktop. Telemetry comes back as a flat
`Float32Array`/`[f32; 264]` (every 12 blocks) into a signal the panel draws
from.

## Inspiration and sources

- **Frekussion FK-2**, the FM percussion synthesizer this is built on, and
  **Xenostasis XS-1**, the same machine built on the Xenokussion XK-1
- Nasca Octavian Paul, **PaulStretch** and PaulXStretch: extreme time
  stretching with large windows and scrambled phases, plus spectral
  processing (pitch shift, octave mixer, spread, harmonics, tonal/noise).
  Csound's opcode: <https://csound.com/docs/manual/paulstretch.html>, and a
  write-up of the plug-in: <https://sonicfield.org/new-version-of-paulxstretch-an-algorithm-for-transcending-time/>
- **ValhallaShimmer** (pitch shifting in the reverb feedback, +12 st and
  feedback ≥ 0.5 for the Eno/Lanois sound): <https://valhalladsp.com/2010/08/30/introducing-valhallashimmer/>
- **Roland RE-201 Space Echo**: <https://musicradar.com/news/producers-guide-roland-space-echo>
- Drone synths: <https://www.signalsounds.com/blog/buyers-guide-drone-synths>,
  <https://modeaudio.com/magazine/creating-drones-with-synths>
- Ambient and dub techno drones: <https://www.ableton.com/zh-cn/blog/drone-tutorials/>
- G. Toussaint, "The Euclidean Algorithm Generates Traditional Musical
  Rhythms" (2005), and E. Bjorklund's timing-system algorithm
- J. Dattorro, "Effect Design, Part 1: Reverberator and Other Filters", JAES
  45(9), 1997 (the plate)
- O. Niemitalo, IIR Hilbert transformer allpass pairs (the SPIRAL frequency
  shifter)
