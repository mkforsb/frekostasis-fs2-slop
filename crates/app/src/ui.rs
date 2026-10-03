//! The panel: transport, pattern ring and stretch up top, the STRATA view of
//! what the machine is doing, then the source (the headless FK-2), spectral,
//! echo and reverb sections.

use dioxus::prelude::*;
use frekostasis_dsp::clock::Pattern;
use frekostasis_dsp::params::{Param, ParamKind, map};
use frekostasis_dsp::presets::SCENES;
use frekostasis_dsp::source::OUTLINE;
use frekostasis_dsp::stretch::{MAX_LAYERS, SPECTRUM_BANDS};
use frekostasis_dsp::telemetry as t;
use frekussion_dsp::OPS;
use frekussion_dsp::params::ALGORITHMS;
use frekussion_dsp::presets::KITS;

use crate::audio::AudioStatus;
use crate::state::{Drag, Synth, nudge_step};

const STYLE: &str = include_str!("../assets/style.css");

/// Pixels of vertical drag for a knob's full travel.
const KNOB_SPAN_PX: f64 = 200.0;
/// Cells of the pool display.
const POOL_CELLS: usize = frekostasis_dsp::clock::POOL_TARGET_MAX;
/// Hue of each FK-2 algorithm, for the layer strata: deep stacks pink,
/// through the branches and pairs, to fully additive teal.
const ALGO_HUES: [u32; 8] = [330, 345, 5, 30, 42, 200, 185, 172];

#[component]
pub fn App() -> Element {
    let synth = use_context_provider(Synth::new);
    let drag = synth.drag;

    let s_move = synth.clone();
    let s_up = synth.clone();
    let s_down = synth.clone();
    let s_key = synth.clone();
    let end_drag = move || {
        let mut drag = s_up.drag;
        if drag.peek().is_some() {
            drag.set(None);
            s_up.save();
        }
    };
    let end_drag_leave = end_drag.clone();
    let end_drag_up = end_drag;

    rsx! {
        style { {STYLE} }
        div {
            class: if drag.read().is_some() { "root dragging" } else { "root" },
            tabindex: "0",
            autofocus: true,
            onpointerdown: move |_| s_down.audio.user_gesture(),
            onpointermove: move |e: PointerEvent| {
                let Some(d) = *s_move.drag.peek() else { return };
                e.prevent_default();
                let y = e.client_coordinates().y;
                let fine = e.modifiers().shift();
                let mut drag = s_move.drag;
                if fine != d.fine {
                    // Re-anchor so toggling SHIFT mid-drag doesn't jump.
                    drag.set(Some(Drag { start_y: y, start_pos: s_move.pos(d.param), fine, ..d }));
                    return;
                }
                let scale = if fine { 0.1 } else { 1.0 };
                let pos = d.start_pos as f64 + (d.start_y - y) / d.span_px * scale;
                s_move.set_pos(d.param, pos as f32);
            },
            onpointerup: move |_| end_drag_up(),
            onpointerleave: move |_| end_drag_leave(),
            onkeydown: move |e: KeyboardEvent| {
                let m = e.modifiers();
                if e.is_auto_repeating() || m.ctrl() || m.alt() || m.meta() {
                    return;
                }
                match e.code() {
                    Code::Space => {
                        e.prevent_default();
                        s_key.toggle_playing();
                    }
                    Code::KeyF => {
                        e.prevent_default();
                        s_key.toggle(Param::Freeze);
                    }
                    Code::KeyX => {
                        e.prevent_default();
                        s_key.strike();
                    }
                    _ => s_key.audio.user_gesture(),
                }
            },
            Header {}
            main { class: "panel",
                div { class: "hero",
                    Transport {}
                    PatternCard {}
                    StretchCard {}
                }
                Strata {}
                div { class: "decks",
                    SourceCard {}
                    SpectralCard {}
                }
                div { class: "decks",
                    EchoCard {}
                    ReverbCard {}
                }
            }
            footer { class: "footer",
                kbd { "Space" }
                span { " play / stop · " }
                kbd { "F" }
                span { " freeze · " }
                kbd { "X" }
                span { " strike now · Drag knobs vertically (" }
                kbd { "Shift" }
                span { " = fine), scroll to nudge, double-click to reset. Algorithms: click to toggle, double-click to solo; " }
                span { class: "legend car", "carriers" }
                span { " and " }
                span { class: "legend mod", "modulators" }
                span { " as on the FK-2." }
            }
        }
    }
}

#[component]
fn Header() -> Element {
    let synth = use_context::<Synth>();
    let status = synth.audio.status.read().clone();
    let (class, text) = match status {
        AudioStatus::NeedsGesture => (
            "status warn",
            "Press PLAY (or any key) to start audio".to_string(),
        ),
        AudioStatus::Starting => ("status warn", "Starting audio…".to_string()),
        AudioStatus::Running {
            sample_rate,
            detail,
        } => (
            "status ok",
            format!("{detail} · {:.1} kHz", sample_rate as f32 / 1000.0),
        ),
        AudioStatus::Failed(e) => ("status err", format!("Audio unavailable: {e}")),
    };
    let s_scene = synth.clone();
    rsx! {
        header { class: "header",
            div { class: "brand",
                Logo {}
                span { class: "logo", "FREKOSTASIS" }
                span { class: "model", "FS-2" }
                span { class: "tagline", "Extreme-Stretch FM Drone Machine" }
            }
            select {
                class: "scene",
                title: "Load a scene",
                onchange: move |e| {
                    if let Ok(i) = e.value().parse::<usize>() {
                        s_scene.load_scene(i);
                    }
                },
                option { value: "", disabled: true, selected: true, "Scene…" }
                for (i, s) in SCENES.iter().enumerate() {
                    option { key: "{i}", value: "{i}", "{s.name}" }
                }
            }
            div { class: "{class}",
                span { class: "status-dot" }
                span { "{text}" }
            }
            Meter {}
            Knob { param: Param::Master, size: "sm" }
        }
    }
}

/// The brand mark: a phase-modulated sine whose index swells and relaxes, a
/// strike stretched into a slow FM bloom.
#[component]
fn Logo() -> Element {
    let path = use_hook(|| {
        let mut d = String::new();
        for i in 0..=160 {
            let x = i as f64 / 160.0;
            let env = (-(x - 0.55).powi(2) / 0.09).exp();
            let index = 2.8 * env;
            let tau = std::f64::consts::TAU;
            let wobble = (tau * 7.0 * x + index * (tau * 21.0 * x).sin()).sin();
            let y = 13.0 - (0.25 + 0.75 * env) * wobble * 10.0;
            d += &format!("{}{:.1} {:.1}", if i == 0 { 'M' } else { 'L' }, x * 40.0, y);
        }
        d
    });
    rsx! {
        svg { class: "brand-mark", view_box: "0 0 40 26",
            path { d: "{path}" }
        }
    }
}

/// Output peak meter from the telemetry.
#[component]
fn Meter() -> Element {
    let synth = use_context::<Synth>();
    let t = synth.audio.telemetry.read();
    let db = |v: f32| ((20.0 * v.max(1e-5).log10() + 48.0) / 48.0).clamp(0.0, 1.0) * 100.0;
    let (l, r) = (db(t[t::PEAK_L]), db(t[t::PEAK_R]));
    rsx! {
        div { class: "meter", title: "Output level",
            div { class: "meter-bar", div { class: "meter-fill", style: "width: {l:.1}%" } }
            div { class: "meter-bar", div { class: "meter-fill", style: "width: {r:.1}%" } }
        }
    }
}

// --- hero row ---------------------------------------------------------------

#[component]
fn Transport() -> Element {
    let synth = use_context::<Synth>();
    let playing = *synth.playing.read();
    let t = synth.audio.telemetry.read();
    let strikes = t[t::STRIKES] as u32;
    let layers = t[t::LAYERS] as u32;
    let frozen = synth.value(Param::Freeze) > 0.5;
    let s_play = synth.clone();
    let s_strike = synth.clone();
    let s_freeze = synth.clone();
    rsx! {
        section { class: "card transport",
            div { class: "card-title", "TRANSPORT" }
            button {
                class: if playing { "play on" } else { "play" },
                title: "Play / stop (Space)",
                onclick: move |_| s_play.toggle_playing(),
                svg { class: "play-glyph", view_box: "0 0 24 24",
                    if playing {
                        rect { x: "6", y: "6", width: "12", height: "12", rx: "1.5" }
                    } else {
                        path { d: "M8 5.5v13l11-6.5z" }
                    }
                }
                span { class: "play-label", if playing { "STOP" } else { "PLAY" } }
            }
            div { class: "transport-row",
                button {
                    class: "btn strike",
                    title: "Strike a new hit now (X)",
                    disabled: !playing,
                    onclick: move |_| s_strike.strike(),
                    for k in std::iter::once(strikes) {
                        span { key: "{k}", class: if k > 0 { "led flash" } else { "led" } }
                    }
                    "STRIKE"
                }
                button {
                    class: if frozen { "btn freeze on" } else { "btn freeze" },
                    title: "Freeze time: hold every layer where it is (F)",
                    onclick: move |_| s_freeze.toggle(Param::Freeze),
                    span { class: "led" }
                    "FREEZE"
                }
            }
            div { class: "transport-stats",
                span { "STRIKES " b { "{strikes}" } }
                span { "LAYERS " b { "{layers}" } }
            }
        }
    }
}

#[component]
fn PatternCard() -> Element {
    rsx! {
        section { class: "card pattern",
            div { class: "card-title", "PATTERN" }
            div { class: "pattern-body",
                PatternRing {}
                div { class: "knob-grid",
                    Knob { param: Param::StepTime }
                    Knob { param: Param::Chance }
                    Knob { param: Param::Steps, size: "sm" }
                    Knob { param: Param::Pulses, size: "sm" }
                    Knob { param: Param::Rotate, size: "sm" }
                }
            }
        }
    }
}

/// The Euclidean pattern as a ring of steps, with a hand sweeping through
/// the current step while playing.
#[component]
fn PatternRing() -> Element {
    let synth = use_context::<Synth>();
    let patch = synth.state.read().patch;
    let pattern = Pattern::from_patch(&patch);
    let t = synth.audio.telemetry.read();
    let live = *synth.playing.read() && t[t::PLAYING] > 0.5 && t[t::FROZEN] < 0.5;
    let n = pattern.steps as usize;
    let step = (t[t::STEP] as usize).min(n.saturating_sub(1));
    let (cx, cy, r) = (100.0f64, 100.0f64, 78.0f64);
    let at = |turns: f64, radius: f64| {
        let a = turns * std::f64::consts::TAU;
        (cx + radius * a.sin(), cy - radius * a.cos())
    };
    let dots: Vec<(f64, f64, bool, bool)> = (0..n)
        .map(|i| {
            let (x, y) = at(i as f64 / n as f64, r);
            (x, y, pattern.is_pulse(i as u32), live && i == step)
        })
        .collect();
    let (hx, hy) = at((step as f64 + t[t::STEP_PHASE] as f64) / n as f64, r - 16.0);
    // Polygon through the pulses, like a Euclidean rhythm necklace.
    let poly: String = dots
        .iter()
        .filter(|d| d.2)
        .map(|(x, y, _, _)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ");
    let strikes = t[t::STRIKES] as u32;
    let secs = map::step_seconds(patch[Param::StepTime.index()]);
    let cycle = secs * n as f32;
    rsx! {
        svg { class: "ring", view_box: "0 0 200 200",
            circle { class: "ring-track", cx: "{cx}", cy: "{cy}", r: "{r}" }
            if pattern.pulses > 1 {
                polygon { class: "ring-poly", points: "{poly}" }
            }
            for k in std::iter::once(strikes) {
                circle { key: "{k}", class: if k > 0 { "ring-flash on" } else { "ring-flash" }, cx: "{cx}", cy: "{cy}", r: "{r}" }
            }
            if live {
                line { class: "ring-hand", x1: "{cx}", y1: "{cy}", x2: "{hx:.1}", y2: "{hy:.1}" }
            }
            for (i, (x, y, pulse, current)) in dots.into_iter().enumerate() {
                circle {
                    key: "{i}",
                    class: match (pulse, current) {
                        (true, true) => "step pulse current",
                        (true, false) => "step pulse",
                        (false, true) => "step current",
                        (false, false) => "step",
                    },
                    cx: "{x:.1}",
                    cy: "{y:.1}",
                    r: if pulse { "7" } else { "3.5" },
                }
            }
            text { class: "ring-text", x: "{cx}", y: "{cy - 4.0}", "E({pattern.pulses},{pattern.steps})" }
            text { class: "ring-sub", x: "{cx}", y: "{cy + 14.0}", "cycle {frekostasis_dsp::params::fmt_secs(cycle)}" }
        }
    }
}

#[component]
fn StretchCard() -> Element {
    let synth = use_context::<Synth>();
    let patch = synth.state.read().patch;
    let factor = map::stretch_factor(patch[Param::Stretch.index()]);
    let t = synth.audio.telemetry.read();
    let avg = t[t::HIT_AVERAGE];
    let example = if avg > 0.0 { avg } else { 1.0 };
    let stretched = frekostasis_dsp::params::fmt_secs(example * factor);
    let source = frekostasis_dsp::params::fmt_secs(example);
    rsx! {
        section { class: "card stretch",
            div { class: "card-title", "STRETCH" }
            div { class: "stretch-body",
                Knob { param: Param::Stretch, size: "xl" }
                div { class: "stretch-side",
                    div { class: "dilation",
                        span { class: "dil-src", "{source}" }
                        span { class: "dil-arrow", "⟶" }
                        span { class: "dil-dst", "{stretched}" }
                    }
                    div { class: "dil-note", "a typical hit, stretched" }
                    Knob { param: Param::Width }
                }
            }
            Segmented { param: Param::Window }
        }
    }
}

// --- strata -----------------------------------------------------------------

/// What the machine is doing: the spectrum of the drone, one band of
/// strata per stretched layer, the pool of rendered hits and the last strike.
#[component]
fn Strata() -> Element {
    let synth = use_context::<Synth>();
    let t = *synth.audio.telemetry.read();

    // Spectrum as a filled ridge.
    let (w, h) = (480.0f32, 120.0f32);
    let bx = |i: usize| (i as f32 + 0.5) / SPECTRUM_BANDS as f32 * w;
    let by = |v: f32| h - 4.0 - v * (h - 10.0);
    let bands = &t[t::SPECTRUM_BASE..t::SPECTRUM_BASE + SPECTRUM_BANDS];
    let mut ridge = format!("M0 {h} L0 {:.1}", by(bands[0]));
    for (i, &v) in bands.iter().enumerate() {
        ridge += &format!(" L{:.1} {:.1}", bx(i), by(v));
    }
    ridge += &format!(" L{w} {:.1} L{w} {h} Z", by(bands[SPECTRUM_BANDS - 1]));

    let count = (t[t::LAYERS] as usize).min(MAX_LAYERS);
    let layers: Vec<(f32, f32, u32, u32)> = (0..count)
        .map(|i| {
            let b = t::LAYER_BASE + i * t::LAYER_FIELDS;
            (t[b], t[b + 1], t[b + 2] as u32, t[b + 3] as u32)
        })
        .collect();

    let pool = t[t::POOL] as usize;
    let fresh = t[t::POOL_FRESH] as usize;
    let target = t[t::POOL_TARGET] as usize;
    let rendering = t[t::RENDERING];

    let mut outline = String::new();
    let (ow, oh) = (OUTLINE as f32, 40.0f32);
    let peak = (0..OUTLINE)
        .map(|i| t[t::OUTLINE_BASE + 2 * i + 1].max(-t[t::OUTLINE_BASE + 2 * i]))
        .fold(1e-3f32, f32::max);
    for i in 0..OUTLINE {
        let hi = t[t::OUTLINE_BASE + 2 * i + 1] / peak;
        outline += &format!(
            "{}{} {:.1}",
            if i == 0 { 'M' } else { 'L' },
            i,
            oh / 2.0 - hi * oh * 0.45
        );
    }
    for i in (0..OUTLINE).rev() {
        let lo = t[t::OUTLINE_BASE + 2 * i] / peak;
        outline += &format!("L{} {:.1}", i, oh / 2.0 - lo * oh * 0.45);
    }
    outline += "Z";
    let has_hit = t[t::HIT_SECONDS] > 0.0;
    let routing = |a: f32| ALGORITHMS.get(a as usize).map_or("?", |a| a.name);
    let hit_text = format!(
        "{} {} + {} {} · {}",
        routing(t[t::HIT_ALGO_A]),
        frekostasis_dsp::params::fmt_hz(t[t::HIT_PITCH_A]),
        routing(t[t::HIT_ALGO_B]),
        frekostasis_dsp::params::fmt_hz(t[t::HIT_PITCH_B]),
        frekostasis_dsp::params::fmt_secs(t[t::HIT_SECONDS]),
    );
    let hop_ms = t[t::HOP_SECONDS] * 1000.0;
    let starved = t[t::STARVED] as u32;

    rsx! {
        section { class: "card strata",
            div { class: "strata-spectrum",
                div { class: "card-title", "SPECTRUM" }
                svg { class: "spectrum", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
                    defs {
                        linearGradient { id: "aurora", x1: "0", y1: "0", x2: "1", y2: "0",
                            stop { offset: "0%", "stop-color": "#1f9e8f" }
                            stop { offset: "40%", "stop-color": "#2fd6c3" }
                            stop { offset: "75%", "stop-color": "#ff5ea8" }
                            stop { offset: "100%", "stop-color": "#ffb547" }
                        }
                    }
                    for i in 1..8 {
                        line { key: "{i}", class: "grid", x1: "0", x2: "{w}", y1: "{i as f32 * h / 8.0}", y2: "{i as f32 * h / 8.0}" }
                    }
                    path { class: "ridge", d: "{ridge}" }
                }
                div { class: "axis",
                    span { "30 Hz" }
                    span { "250" }
                    span { "2k" }
                    span { "16 kHz" }
                }
            }
            div { class: "strata-layers",
                div { class: "card-title",
                    "STRATA"
                    span { class: "meta", "{count} layers · hop {hop_ms:.0} ms" }
                }
                div { class: "layers",
                    if layers.is_empty() {
                        div { class: "layers-empty", "no layers — press PLAY" }
                    }
                    for (i, (progress, gain, a, b)) in layers.into_iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "layer",
                            title: "{routing(a as f32)} + {routing(b as f32)} · {progress * 100.0:.0}% read",
                            style: "--h1: {ALGO_HUES[a as usize % 8]}; --h2: {ALGO_HUES[b as usize % 8]}; --g: {gain:.2}",
                            div { class: "layer-read", style: "width: {progress * 100.0:.1}%" }
                            div { class: "layer-head", style: "left: {progress * 100.0:.1}%" }
                        }
                    }
                }
            }
            div { class: "strata-pool",
                div { class: "card-title",
                    "POOL"
                    span { class: "meta", "{fresh}/{target}" }
                }
                div { class: "pool",
                    for i in 0..POOL_CELLS {
                        div {
                            key: "{i}",
                            class: if i < fresh {
                                "cell fresh"
                            } else if i < pool {
                                "cell stale"
                            } else if i < target {
                                "cell wanted"
                            } else {
                                "cell"
                            },
                        }
                    }
                }
                div { class: "render-bar", div { class: "render-fill", style: "width: {rendering * 100.0:.0}%" } }
                div { class: "pool-note",
                    if rendering > 0.0 { "rendering FK-2 strike…" } else { "pool full" }
                    if starved > 0 { " · {starved} re-used" }
                }
                div { class: "card-title last-title", "LAST STRIKE" }
                svg { class: "outline", view_box: "0 0 {ow} {oh}", preserve_aspect_ratio: "none",
                    if has_hit {
                        path { d: "{outline}" }
                    }
                }
                div { class: "hit-text", if has_hit { "{hit_text}" } else { "—" } }
            }
        }
    }
}

// --- decks -------------------------------------------------------------------

#[component]
fn SourceCard() -> Element {
    let synth = use_context::<Synth>();
    let origin = synth.value(Param::Origin) as usize;
    let s_origin = synth.clone();
    rsx! {
        section { class: "card source",
            div { class: "card-title",
                "SOURCE"
                span { class: "meta", "headless FREKUSSION FK-2 · both channels" }
            }
            AlgorithmToggles {}
            div { class: "row",
                Knob { param: Param::Mutate, size: "lg" }
                div { class: "origin",
                    div { class: "ctl-label", "ORIGIN" }
                    select {
                        class: "select",
                        title: "FK-2 kit the two channels start (and restart) from",
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                s_origin.set(Param::Origin, i as f32);
                                s_origin.save();
                            }
                        },
                        for (i, k) in KITS.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i == origin, "{k.name}" }
                        }
                    }
                    div { class: "hint", "MUTATE walks away from the kit on every strike" }
                }
                Knob { param: Param::Length }
                Knob { param: Param::Grit }
            }
            div { class: "row",
                Knob { param: Param::Pitch }
                Knob { param: Param::Range }
                Knob { param: Param::Root, size: "sm" }
                Knob { param: Param::Index }
                Knob { param: Param::Cross }
            }
            div { class: "seg-row",
                Segmented { param: Param::Scale }
                Segmented { param: Param::Ratios }
            }
        }
    }
}

/// The eight FK-2 algorithms MUTATE may pick from, each drawn as its
/// routing.
#[component]
fn AlgorithmToggles() -> Element {
    let synth = use_context::<Synth>();
    let mask = synth.value(Param::Algorithms) as u32;
    rsx! {
        div { class: "algos",
            for i in 0..ALGORITHMS.len() {
                AlgorithmButton { key: "{i}", index: i, on: mask & (1 << i) != 0 }
            }
        }
    }
}

/// Where each operator box sits in an algorithm diagram (viewBox 100 × 100,
/// operator 1 first), as on the FK-2's panel.
const ALGO_LAYOUT: [[(f64, f64); OPS]; 8] = [
    [(50.0, 76.0), (50.0, 54.0), (50.0, 32.0), (50.0, 10.0)],
    [(50.0, 76.0), (50.0, 50.0), (32.0, 22.0), (68.0, 22.0)],
    [(50.0, 76.0), (30.0, 46.0), (70.0, 46.0), (70.0, 18.0)],
    [(50.0, 76.0), (20.0, 42.0), (50.0, 42.0), (80.0, 42.0)],
    [(30.0, 76.0), (30.0, 44.0), (70.0, 76.0), (70.0, 44.0)],
    [(20.0, 76.0), (50.0, 76.0), (80.0, 76.0), (50.0, 42.0)],
    [(18.0, 76.0), (18.0, 44.0), (50.0, 76.0), (82.0, 76.0)],
    [(14.0, 76.0), (38.0, 76.0), (62.0, 76.0), (86.0, 76.0)],
];
const OP_BOX_W: f64 = 18.0;
const OP_BOX_H: f64 = 13.0;
const OUT_Y: f64 = 95.0;

#[component]
fn AlgorithmButton(index: usize, on: bool) -> Element {
    let synth = use_context::<Synth>();
    let s_dbl = synth.clone();
    let algo = ALGORITHMS[index];
    let layout = ALGO_LAYOUT[index];
    let mut edges = Vec::new();
    for to in 0..OPS {
        for from in 0..OPS {
            if algo.modulates(from, to) {
                let (x0, y0) = layout[from];
                let (x1, y1) = layout[to];
                edges.push((x0, y0 + OP_BOX_H / 2.0, x1, y1 - OP_BOX_H / 2.0));
            }
        }
    }
    let carriers: Vec<f64> = (0..OPS)
        .filter(|&op| algo.is_carrier(op))
        .map(|op| layout[op].0)
        .collect();
    let out_x0 = carriers.iter().cloned().fold(f64::MAX, f64::min);
    let out_x1 = carriers.iter().cloned().fold(f64::MIN, f64::max);
    rsx! {
        button {
            class: if on { "algo on" } else { "algo" },
            style: "--h: {ALGO_HUES[index]}",
            "aria-pressed": "{on}",
            title: "Algorithm {index + 1}: {algo.name} (click to toggle, double-click to solo)",
            onclick: move |_| synth.toggle_algorithm(index),
            ondoubleclick: move |_| s_dbl.solo_algorithm(index),
            svg { class: "algo-svg", view_box: "0 0 100 100",
                for (i, (x0, y0, x1, y1)) in edges.into_iter().enumerate() {
                    line { key: "{i}", class: "algo-edge", x1: "{x0}", y1: "{y0}", x2: "{x1}", y2: "{y1}" }
                }
                for (i, x) in carriers.iter().enumerate() {
                    line { key: "c{i}", class: "algo-out", x1: "{x}", y1: "{76.0 + OP_BOX_H / 2.0}", x2: "{x}", y2: "{OUT_Y}" }
                }
                line { class: "algo-out", x1: "{out_x0 - 4.0}", y1: "{OUT_Y}", x2: "{out_x1 + 4.0}", y2: "{OUT_Y}" }
                for op in 0..OPS {
                    rect {
                        key: "{op}",
                        class: if algo.is_carrier(op) { "algo-op car" } else { "algo-op mod" },
                        x: "{layout[op].0 - OP_BOX_W / 2.0}",
                        y: "{layout[op].1 - OP_BOX_H / 2.0}",
                        width: "{OP_BOX_W}",
                        height: "{OP_BOX_H}",
                        rx: "2",
                    }
                }
            }
            span { "{index + 1}" }
        }
    }
}

#[component]
fn SpectralCard() -> Element {
    rsx! {
        section { class: "card spectral",
            div { class: "card-title", "SPECTRAL" }
            div { class: "row",
                Knob { param: Param::Shift }
                Knob { param: Param::Sub }
                Knob { param: Param::Smear }
            }
            div { class: "row",
                Knob { param: Param::Focus }
                Knob { param: Param::Lock }
                Knob { param: Param::Tilt }
            }
        }
    }
}

#[component]
fn EchoCard() -> Element {
    let synth = use_context::<Synth>();
    let on = synth.value(Param::EchoOn) > 0.5;
    rsx! {
        section { class: if on { "card echo" } else { "card echo off" },
            div { class: "card-title",
                "SPACE ECHO"
                Toggle { param: Param::EchoOn }
            }
            div { class: "row",
                Knob { param: Param::EchoTime }
                Toggle { param: Param::EchoSync, label: "SYNC" }
                Knob { param: Param::EchoFeedback }
                Knob { param: Param::EchoTone }
                Knob { param: Param::EchoLevel }
            }
            div { class: "seg-row",
                Segmented { param: Param::EchoMode }
                Segmented { param: Param::EchoSpacing }
            }
        }
    }
}

#[component]
fn ReverbCard() -> Element {
    let synth = use_context::<Synth>();
    let on = synth.value(Param::VerbOn) > 0.5;
    rsx! {
        section { class: if on { "card reverb" } else { "card reverb off" },
            div { class: "card-title",
                "SHIMMER PLATE"
                Toggle { param: Param::VerbOn }
            }
            div { class: "row",
                Knob { param: Param::VerbMix }
                Knob { param: Param::VerbDecay }
                Knob { param: Param::VerbTone }
                Knob { param: Param::VerbPre }
                Knob { param: Param::Shimmer }
            }
            Segmented { param: Param::ShimmerInterval }
        }
    }
}

// --- widgets ----------------------------------------------------------------

fn polar(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let a = deg.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

fn arc_path(cx: f64, cy: f64, r: f64, from_deg: f64, to_deg: f64) -> String {
    let (x0, y0) = polar(cx, cy, r, from_deg);
    let (x1, y1) = polar(cx, cy, r, to_deg);
    let large = if (to_deg - from_deg).abs() > 180.0 {
        1
    } else {
        0
    };
    format!("M {x0:.2} {y0:.2} A {r} {r} 0 {large} 1 {x1:.2} {y1:.2}")
}

fn begin_drag(synth: &Synth, param: Param, e: &PointerEvent, span_px: f64) {
    e.prevent_default();
    synth.audio.user_gesture();
    let mut touched = synth.touched;
    touched.set(Some(param));
    let mut drag = synth.drag;
    drag.set(Some(Drag {
        param,
        start_y: e.client_coordinates().y,
        start_pos: synth.pos(param),
        span_px,
        fine: e.modifiers().shift(),
    }));
}

fn nudge(synth: &Synth, param: Param, e: &WheelEvent) {
    e.prevent_default();
    let dy = e.delta().strip_units().y;
    if dy == 0.0 {
        return;
    }
    let step = nudge_step(param, e.modifiers().shift());
    synth.set_pos(param, synth.pos(param) - (dy.signum() as f32) * step);
    synth.save();
}

#[component]
fn Knob(param: Param, #[props(default = "lg")] size: &'static str) -> Element {
    let synth = use_context::<Synth>();
    let patch = synth.state.read().patch;
    let value = patch[param.index()];
    let pos = param.kind().to_norm(value);
    let label = param.label(&patch);
    let name = param.info().name;
    let text = param.display(value, &patch);
    let active = matches!(*synth.drag.read(), Some(d) if d.param == param);
    let angle = -135.0 + 270.0 * pos as f64;
    let (px, py) = polar(30.0, 30.0, 15.0, angle);
    let track = arc_path(30.0, 30.0, 26.0, -135.0, 135.0);
    let (a0, a1) = if param.is_bipolar() {
        (angle.min(0.0), angle.max(0.0))
    } else {
        (-135.0, angle)
    };
    let lit = if (a1 - a0).abs() > 0.5 {
        arc_path(30.0, 30.0, 26.0, a0, a1)
    } else {
        String::new()
    };
    let stepped = !matches!(param.kind(), ParamKind::Continuous);
    let ticks: Vec<(f64, f64, f64, f64)> = match param.kind() {
        ParamKind::Int { min, max } if max - min <= 16 => (0..=(max - min))
            .map(|i| {
                let a = -135.0 + 270.0 * i as f64 / (max - min) as f64;
                let (x0, y0) = polar(30.0, 30.0, 23.0, a);
                let (x1, y1) = polar(30.0, 30.0, 29.0, a);
                (x0, y0, x1, y1)
            })
            .collect(),
        _ => vec![],
    };
    let s_down = synth.clone();
    let s_dbl = synth.clone();
    let s_wheel = synth.clone();
    rsx! {
        div {
            class: "knob knob-{size}",
            class: if active { "active" },
            class: if stepped { "stepped" },
            div { class: "ctl-label", "{label}" }
            svg {
                class: "knob-svg",
                view_box: "0 0 60 60",
                role: "slider",
                "aria-label": "{name}",
                "aria-valuetext": "{text}",
                onpointerdown: move |e| begin_drag(&s_down, param, &e, KNOB_SPAN_PX),
                ondoubleclick: move |_| s_dbl.reset(param),
                onwheel: move |e| nudge(&s_wheel, param, &e),
                title { "{name}: {text}" }
                path { class: "knob-track", d: "{track}" }
                for (i, (x0, y0, x1, y1)) in ticks.into_iter().enumerate() {
                    line { key: "{i}", class: "knob-tick", x1: "{x0:.2}", y1: "{y0:.2}", x2: "{x1:.2}", y2: "{y1:.2}" }
                }
                if !lit.is_empty() {
                    path { class: "knob-lit", d: "{lit}" }
                }
                circle { class: "knob-skirt", cx: "30", cy: "30", r: "21" }
                circle { class: "knob-body", cx: "30", cy: "30", r: "17" }
                line { class: "knob-pointer", x1: "30", y1: "30", x2: "{px:.2}", y2: "{py:.2}" }
            }
            div { class: "ctl-value", "{text}" }
        }
    }
}

/// A row of buttons for a choice parameter.
#[component]
fn Segmented(param: Param) -> Element {
    let synth = use_context::<Synth>();
    let ParamKind::Choice(names) = param.kind() else {
        return rsx! {};
    };
    let current = synth.value(param) as usize;
    rsx! {
        div { class: "seg-wrap",
            div { class: "ctl-label", "{param.info().label}" }
            div { class: "seg", role: "radiogroup", "aria-label": "{param.info().name}",
                for (i, name) in names.iter().enumerate() {
                    SegButton { key: "{i}", param, index: i, name, selected: i == current }
                }
            }
        }
    }
}

#[component]
fn SegButton(param: Param, index: usize, name: &'static str, selected: bool) -> Element {
    let synth = use_context::<Synth>();
    rsx! {
        button {
            class: if selected { "seg-btn sel" } else { "seg-btn" },
            role: "radio",
            "aria-checked": "{selected}",
            onclick: move |_| {
                synth.set(param, index as f32);
                synth.save();
            },
            "{name}"
        }
    }
}

/// LED push button for an on/off parameter.
#[component]
fn Toggle(param: Param, #[props(default = "")] label: &'static str) -> Element {
    let synth = use_context::<Synth>();
    let on = synth.value(param) > 0.5;
    let text = if label.is_empty() {
        if on { "ON" } else { "OFF" }
    } else {
        label
    };
    rsx! {
        button {
            class: if on { "toggle on" } else { "toggle" },
            "aria-pressed": "{on}",
            title: "{param.info().name}",
            onclick: move |_| synth.toggle(param),
            span { class: "led" }
            "{text}"
        }
    }
}
