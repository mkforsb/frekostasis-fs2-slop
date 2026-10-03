//! PulseAudio backend (works with PipeWire's pulse server too).
//!
//! A dedicated thread owns the machine and a `pa_simple` playback stream.
//! The UI sends [`Command`]s over a channel; the thread drains it before
//! every block and reports telemetry back every [`TELEMETRY_BLOCKS`].

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use dioxus::prelude::*;
use frekostasis_dsp::Machine;
use frekostasis_dsp::telemetry::LEN as TELEMETRY_LEN;
use libpulse_binding::def::BufferAttr;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;

use super::{AudioStatus, Command, ParamCache, TELEMETRY_BLOCKS, TelemetryTx};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u8 = 2;
const BLOCK: usize = 128;
const BYTES_PER_FRAME: usize = 4 * CHANNELS as usize;
/// A drone doesn't need low latency; generous buffering keeps it glitch-free.
/// Override with `FREKOSTASIS_LATENCY_MS`.
const DEFAULT_LATENCY_MS: u32 = 60;

pub struct PulseBackend {
    tx: mpsc::Sender<Command>,
}

impl PulseBackend {
    pub fn new(
        mut status: Signal<AudioStatus>,
        initial: ParamCache,
        telemetry: TelemetryTx,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
        let latency_ms = std::env::var("FREKOSTASIS_LATENCY_MS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_LATENCY_MS)
            .clamp(5, 1000);

        let spawned = thread::Builder::new()
            .name("frekostasis-audio".into())
            .spawn(move || match open_stream(latency_ms) {
                Ok(stream) => {
                    let _ =
                        ready_tx.send(Ok(format!("PulseAudio · {latency_ms} ms target latency")));
                    run(stream, rx, initial, telemetry);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            });

        let result = match spawned {
            Err(e) => Err(format!("could not spawn audio thread: {e}")),
            Ok(_) => ready_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|_| Err("timed out connecting to PulseAudio".into())),
        };
        status.set(match result {
            Ok(detail) => AudioStatus::Running {
                sample_rate: SAMPLE_RATE,
                detail,
            },
            Err(e) => {
                eprintln!("frekostasis: audio unavailable: {e}");
                AudioStatus::Failed(e)
            }
        });
        Self { tx }
    }

    pub fn send(&self, cmd: Command) {
        // If the audio thread died the error is already shown in the status.
        let _ = self.tx.send(cmd);
    }

    pub fn user_gesture(&self) {}
}

fn open_stream(latency_ms: u32) -> Result<Simple, String> {
    let spec = Spec {
        format: Format::F32le,
        channels: CHANNELS,
        rate: SAMPLE_RATE,
    };
    if !spec.is_valid() {
        return Err("invalid sample spec".into());
    }
    let tlength = (SAMPLE_RATE * latency_ms / 1000) * BYTES_PER_FRAME as u32;
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength,
        prebuf: u32::MAX,
        minreq: (BLOCK * BYTES_PER_FRAME) as u32,
        fragsize: u32::MAX,
    };
    Simple::new(
        None,
        "Frekostasis",
        Direction::Playback,
        None,
        "Extreme-stretch drone machine",
        &spec,
        None,
        Some(&attr),
    )
    .map_err(|e| format!("PulseAudio: {e}"))
}

fn seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(12345)
}

fn run(stream: Simple, rx: mpsc::Receiver<Command>, initial: ParamCache, telemetry: TelemetryTx) {
    let mut machine = Machine::new(SAMPLE_RATE as f32, seed());
    for cmd in initial.commands() {
        apply(&mut machine, cmd);
    }
    machine.snap();
    let mut left = [0.0f32; BLOCK];
    let mut right = [0.0f32; BLOCK];
    let mut bytes = vec![0u8; BLOCK * BYTES_PER_FRAME];
    let mut blocks = 0usize;
    loop {
        loop {
            match rx.try_recv() {
                Ok(cmd) => apply(&mut machine, cmd),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        machine.render(&mut left, &mut right);
        for (i, (l, r)) in left.iter().zip(right.iter()).enumerate() {
            bytes[i * 8..i * 8 + 4].copy_from_slice(&l.to_le_bytes());
            bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&r.to_le_bytes());
        }
        blocks += 1;
        if blocks.is_multiple_of(TELEMETRY_BLOCKS) {
            let mut t = Box::new([0.0; TELEMETRY_LEN]);
            machine.telemetry(&mut t);
            let _ = telemetry.unbounded_send(t);
        }
        if let Err(e) = stream.write(&bytes) {
            eprintln!("frekostasis: PulseAudio write failed: {e}");
            return;
        }
    }
}

fn apply(machine: &mut Machine, cmd: Command) {
    match cmd {
        Command::Param { param, value } => machine.set_param(param, value),
        Command::Play(on) => machine.set_playing(on),
        Command::Strike => machine.strike_now(),
    }
}
