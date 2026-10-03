//! C ABI over the machine for the AudioWorkletProcessor (`worklet.js`).
//!
//! The module imports nothing, so the worklet can instantiate it with an
//! empty import object. Each worklet node owns one machine handle.

use frekostasis_dsp::telemetry::{LEN as TELEMETRY_LEN, Telemetry};
use frekostasis_dsp::{Machine, Param};

/// Frames per render call; matches the WebAudio render quantum.
pub const BLOCK: usize = 128;

pub struct Worklet {
    machine: Machine,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    telemetry: Telemetry,
}

#[unsafe(no_mangle)]
pub extern "C" fn fs_new(sample_rate: f32, seed: u32) -> *mut Worklet {
    Box::into_raw(Box::new(Worklet {
        machine: Machine::new(sample_rate, seed),
        left: [0.0; BLOCK],
        right: [0.0; BLOCK],
        telemetry: [0.0; TELEMETRY_LEN],
    }))
}

/// # Safety
/// `w` must come from [`fs_new`] and not have been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_free(w: *mut Worklet) {
    if !w.is_null() {
        drop(unsafe { Box::from_raw(w) });
    }
}

/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_set_param(w: *mut Worklet, id: u32, value: f32) {
    if let Some(p) = Param::from_id(id) {
        unsafe { &mut *w }.machine.set_param(p, value);
    }
}

/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_play(w: *mut Worklet, on: u32) {
    unsafe { &mut *w }.machine.set_playing(on != 0);
}

/// Strike a hit now, outside the pattern.
///
/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_strike(w: *mut Worklet) {
    unsafe { &mut *w }.machine.strike_now();
}

/// Jump smoothed values to their targets (after the initial patch sync).
///
/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_snap(w: *mut Worklet) {
    unsafe { &mut *w }.machine.snap();
}

/// Render `frames` (≤ [`BLOCK`]) into the buffers returned by
/// [`fs_left`]/[`fs_right`].
///
/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_render(w: *mut Worklet, frames: u32) {
    let w = unsafe { &mut *w };
    let n = (frames as usize).min(BLOCK);
    w.machine.render(&mut w.left[..n], &mut w.right[..n]);
}

/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_left(w: *mut Worklet) -> *const f32 {
    unsafe { (*w).left.as_ptr() }
}

/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_right(w: *mut Worklet) -> *const f32 {
    unsafe { (*w).right.as_ptr() }
}

/// Refresh the telemetry array and return a pointer to its
/// [`fs_telemetry_len`] floats.
///
/// # Safety
/// `w` must come from [`fs_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fs_telemetry(w: *mut Worklet) -> *const f32 {
    let w = unsafe { &mut *w };
    w.machine.telemetry(&mut w.telemetry);
    w.telemetry.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn fs_telemetry_len() -> u32 {
    TELEMETRY_LEN as u32
}
