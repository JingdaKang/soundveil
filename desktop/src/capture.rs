//! Where audio comes from.
//!
//! Windows will capture a browser's audio with WASAPI process loopback. Until that exists, and
//! on other systems, a recording can be replayed at live speed to exercise everything after
//! capture.

use soundveil_core::{SAMPLE_RATE, SpeechEvent, SpeechGate};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CHUNK: usize = SAMPLE_RATE / 10;

/// What capture reports. All of it is called on capture's own thread.
pub struct Sink {
    pub events: Box<dyn Fn(Vec<SpeechEvent>) + Send>,
    pub level: Box<dyn Fn(f32) + Send>,
    pub finished: Box<dyn FnOnce() + Send>,
}

pub struct Capture {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn read_recording(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.sample_rate as usize != SAMPLE_RATE || spec.channels != 1 || spec.bits_per_sample != 16
    {
        return Err("Replay needs 16 kHz mono 16-bit audio.".into());
    }
    reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / 32768.0))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}

/// Plays a recording into the pipeline at live speed, then a moment of silence to close it.
pub fn replay(path: &Path, threshold: f32, sink: Sink) -> Result<Capture, String> {
    let mut samples = read_recording(path)?;
    samples.extend(std::iter::repeat_n(0.0, SAMPLE_RATE * 3 / 2));
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let worker = thread::spawn(move || {
        let mut gate = SpeechGate::default();
        gate.threshold = threshold;
        let began = Instant::now();
        for (index, chunk) in samples.chunks(CHUNK).enumerate() {
            if stopping.load(Ordering::Relaxed) {
                return;
            }
            let events = gate.append(chunk);
            if !events.is_empty() {
                (sink.events)(events);
            }
            let power = chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32;
            (sink.level)(power.sqrt());
            let due = began + Duration::from_millis((index as u64 + 1) * 100);
            thread::sleep(due.saturating_duration_since(Instant::now()));
        }
        (sink.finished)();
    });
    Ok(Capture {
        stop,
        worker: Some(worker),
    })
}
