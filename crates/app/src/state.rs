//! Panel state, the controller shared through the Dioxus context, and
//! persistence (localStorage on the web, `~/.config/frekostasis` on desktop).

use dioxus::prelude::*;
use frekostasis_dsp::params::{ALL_PARAMS, Param, ParamKind, Patch, default_patch};
use frekostasis_dsp::presets::SCENES;

use crate::audio::{AudioHandle, Command, ParamCache};

const FORMAT_HEADER: &str = "frekostasis-state 1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelState {
    pub patch: Patch,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            patch: default_patch(),
        }
    }
}

impl PanelState {
    pub fn serialize(&self) -> String {
        let mut out = format!("{FORMAT_HEADER}\n");
        for p in ALL_PARAMS {
            out += &format!("{p:?} {}\n", self.patch[p.index()]);
        }
        out
    }

    /// Lenient: unknown lines are skipped and missing values keep defaults.
    pub fn deserialize(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim() != FORMAT_HEADER {
            return None;
        }
        let mut state = PanelState::default();
        for line in lines {
            let mut parts = line.split_whitespace();
            let (Some(name), Some(v), None) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let Ok(v) = v.parse::<f32>() else { continue };
            if let Some(p) = ALL_PARAMS.iter().find(|p| format!("{p:?}") == name) {
                state.patch[p.index()] = p.sanitize(v);
            }
        }
        Some(state)
    }
}

/// An in-progress knob or fader drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub param: Param,
    pub start_y: f64,
    /// Knob position (0..1) when the drag (re)started.
    pub start_pos: f32,
    /// Pixels of vertical travel for the full range.
    pub span_px: f64,
    pub fine: bool,
}

/// Everything the controls need, shared via `use_context::<Synth>()`.
#[derive(Clone)]
pub struct Synth {
    pub state: Signal<PanelState>,
    pub audio: AudioHandle,
    pub playing: Signal<bool>,
    pub drag: Signal<Option<Drag>>,
    /// Last touched control, for the read-out.
    pub touched: Signal<Option<Param>>,
}

impl Synth {
    pub fn new() -> Self {
        let initial = load().unwrap_or_default();
        Self {
            audio: AudioHandle::new(ParamCache::new(initial.patch)),
            state: Signal::new(initial),
            playing: Signal::new(false),
            drag: Signal::new(None),
            touched: Signal::new(None),
        }
    }

    pub fn value(&self, p: Param) -> f32 {
        self.state.read().patch[p.index()]
    }

    /// Knob position of a parameter, 0..1.
    pub fn pos(&self, p: Param) -> f32 {
        p.kind().to_norm(self.value(p))
    }

    pub fn set(&self, p: Param, value: f32) {
        let value = p.sanitize(value);
        let mut state = self.state;
        if state.peek().patch[p.index()] != value {
            state.write().patch[p.index()] = value;
            self.audio.send(Command::Param { param: p, value });
        }
        let mut touched = self.touched;
        if *touched.peek() != Some(p) {
            touched.set(Some(p));
        }
    }

    /// Set from a knob position.
    pub fn set_pos(&self, p: Param, pos: f32) {
        self.set(p, p.kind().from_norm(pos.clamp(0.0, 1.0)));
    }

    pub fn reset(&self, p: Param) {
        self.set(p, p.default_value());
        self.save();
    }

    pub fn toggle(&self, p: Param) {
        self.set(p, if self.value(p) > 0.5 { 0.0 } else { 1.0 });
        self.save();
    }

    /// Flip one FK-2 algorithm in the ALGORITHMS mask (at least one stays on).
    pub fn toggle_algorithm(&self, algorithm: usize) {
        let mask = self.value(Param::Algorithms) as u32 ^ (1 << algorithm);
        if mask & 0xff != 0 {
            self.set(Param::Algorithms, mask as f32);
            self.save();
        }
    }

    /// Only this algorithm, or all of them if it already was the only one.
    pub fn solo_algorithm(&self, algorithm: usize) {
        let only = 1u32 << algorithm;
        let mask = if self.value(Param::Algorithms) as u32 == only {
            0xff
        } else {
            only
        };
        self.set(Param::Algorithms, mask as f32);
        self.save();
    }

    pub fn load_scene(&self, index: usize) {
        let Some(scene) = SCENES.get(index) else {
            return;
        };
        let patch = scene.patch(&self.state.peek().patch);
        for p in ALL_PARAMS {
            self.set(p, patch[p.index()]);
        }
        self.touched.clone().set(None);
        self.save();
    }

    pub fn set_playing(&self, on: bool) {
        self.audio.user_gesture();
        let mut playing = self.playing;
        playing.set(on);
        self.audio.send(Command::Play(on));
    }

    pub fn toggle_playing(&self) {
        let on = !*self.playing.peek();
        self.set_playing(on);
    }

    pub fn strike(&self) {
        self.audio.user_gesture();
        if *self.playing.peek() {
            self.audio.send(Command::Strike);
        }
    }

    pub fn save(&self) {
        save(&self.state.peek());
    }
}

/// Step for the scroll wheel: 2% of the travel, or one notch of a stepped
/// control.
pub fn nudge_step(p: Param, fine: bool) -> f32 {
    match p.kind() {
        ParamKind::Continuous => {
            if fine {
                0.002
            } else {
                0.02
            }
        }
        k => k.to_norm(1.0) - k.to_norm(0.0),
    }
}

// --- persistence -----------------------------------------------------------

#[cfg(all(feature = "web", not(feature = "desktop")))]
const STORAGE_KEY: &str = "frekostasis.state";

#[cfg(all(feature = "web", not(feature = "desktop")))]
fn load() -> Option<PanelState> {
    let storage = web_sys::window()?.local_storage().ok()??;
    PanelState::deserialize(&storage.get_item(STORAGE_KEY).ok()??)
}

#[cfg(all(feature = "web", not(feature = "desktop")))]
fn save(state: &PanelState) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(STORAGE_KEY, &state.serialize());
    }
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn state_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("frekostasis").join("state.txt"))
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn load() -> Option<PanelState> {
    PanelState::deserialize(&std::fs::read_to_string(state_path()?).ok()?)
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn save(state: &PanelState) {
    let Some(path) = state_path() else { return };
    let result = path
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| std::fs::write(&path, state.serialize()));
    if let Err(e) = result {
        eprintln!(
            "frekostasis: could not save state to {}: {e}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        let mut s = PanelState::default();
        s.patch[Param::Stretch.index()] = 0.123;
        s.patch[Param::Algorithms.index()] = 37.0;
        s.patch[Param::Shift.index()] = -7.0;
        let back = PanelState::deserialize(&s.serialize()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn rejects_foreign_text_and_tolerates_garbage() {
        assert!(PanelState::deserialize("hello").is_none());
        let text = format!(
            "{FORMAT_HEADER}\nNope 1\nStretch abc\nSteps 99\nWidth 0.5 extra\nMaster 0.25\n"
        );
        let s = PanelState::deserialize(&text).unwrap();
        assert_eq!(s.patch[Param::Steps.index()], 16.0);
        assert_eq!(s.patch[Param::Master.index()], 0.25);
        assert_eq!(s.patch[Param::Width.index()], Param::Width.default_value());
    }

    #[test]
    fn nudges_move_stepped_controls_one_notch() {
        for p in [Param::Steps, Param::Shift, Param::Scale, Param::Window] {
            let k = p.kind();
            let v = p.default_value();
            let up = p.sanitize(k.from_norm(k.to_norm(v) + nudge_step(p, false)));
            assert_eq!(up, v + 1.0, "{p:?}");
        }
    }
}
