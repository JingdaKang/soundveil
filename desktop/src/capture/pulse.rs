//! Linux capture: a recording stream on the default output's monitor, which hears what the
//! speakers play.
//!
//! This goes through the PulseAudio client library, which also works on PipeWire. The server
//! converts to 16 kHz mono, follows the default output, and keeps sending silence when nothing
//! plays, so none of that is needed here.

use super::Listener;
use libpulse_binding::def::BufferAttr;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use soundveil_core::SAMPLE_RATE;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

const MONITOR: &str = "@DEFAULT_MONITOR@";
const CHUNK_SAMPLES: usize = SAMPLE_RATE / 20;
const RETRY: Duration = Duration::from_secs(1);

fn connect() -> Result<Simple, String> {
    let spec = Spec {
        format: Format::F32le,
        channels: 1,
        rate: SAMPLE_RATE as u32,
    };
    // Asks the server for short fragments, or it may hold audio back for seconds.
    let buffer = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: (CHUNK_SAMPLES * size_of::<f32>()) as u32,
    };
    Simple::new(
        None,
        "Soundveil",
        Direction::Record,
        Some(MONITOR),
        "System audio",
        &spec,
        None,
        Some(&buffer),
    )
    .map_err(|error| format!("Cannot reach PulseAudio or PipeWire: {error}"))
}

pub fn run(listener: &mut Listener, stop: &AtomicBool, opened: Sender<Result<(), String>>) {
    let mut stream = match connect() {
        Ok(stream) => {
            let _ = opened.send(Ok(()));
            stream
        }
        Err(error) => {
            let _ = opened.send(Err(error));
            return;
        }
    };
    let mut bytes = vec![0_u8; CHUNK_SAMPLES * size_of::<f32>()];
    let mut samples = Vec::with_capacity(CHUNK_SAMPLES);
    while !stop.load(Ordering::Relaxed) {
        if stream.read(&mut bytes).is_ok() {
            samples.clear();
            samples.extend(
                bytes
                    .chunks_exact(size_of::<f32>())
                    .map(|raw| f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
            );
            listener.hear(&samples);
            continue;
        }
        // The server restarted. Capture carries on when it comes back.
        listener.status("Lost the audio server. Reconnecting…");
        let lost = Instant::now();
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if let Ok(next) = connect() {
                stream = next;
                listener.gap(lost.elapsed());
                listener.status("Listening");
                break;
            }
            thread::sleep(RETRY);
        }
    }
}
