//! Where audio comes from.
//!
//! Windows captures what the speakers play with WASAPI loopback. A recording can also be replayed
//! at live speed, which exercises everything after capture on any system.

#[cfg(windows)]
mod wasapi;

use soundveil_core::{SAMPLE_RATE, SpeechEvent, SpeechGate};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CHUNK: usize = SAMPLE_RATE / 10;

/// What capture reports. All of it is called on capture's own thread.
pub struct Sink {
    pub events: Box<dyn Fn(Vec<SpeechEvent>) + Send>,
    pub level: Box<dyn Fn(f32) + Send>,
    pub status: Box<dyn Fn(String) + Send>,
    pub finished: Box<dyn FnOnce() + Send>,
}

/// The part every source shares: 16 kHz audio goes in, speech events and a level come out.
pub struct Listener {
    gate: SpeechGate,
    threshold: f32,
    sink: Sink,
    heard: u64,
    level_samples: usize,
    level_power: f32,
}

impl Listener {
    fn new(threshold: f32, sink: Sink) -> Self {
        let mut gate = SpeechGate::default();
        gate.threshold = threshold;
        Self {
            gate,
            threshold,
            sink,
            heard: 0,
            level_samples: 0,
            level_power: 0.0,
        }
    }

    pub fn hear(&mut self, samples: &[f32]) {
        self.heard += samples.len() as u64;
        let events = self.gate.append(samples);
        if !events.is_empty() {
            (self.sink.events)(events);
        }
        self.level_samples += samples.len();
        self.level_power += samples.iter().map(|s| s * s).sum::<f32>();
        if self.level_samples >= CHUNK {
            (self.sink.level)((self.level_power / self.level_samples as f32).sqrt());
            self.level_samples = 0;
            self.level_power = 0.0;
        }
    }

    /// Audio went missing, so whatever was being said can't be joined to what comes next.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn gap(&mut self, lost: Duration) {
        self.heard += (lost.as_secs_f64() * SAMPLE_RATE as f64) as u64;
        self.gate = SpeechGate::new(self.heard as f64 / SAMPLE_RATE as f64);
        self.gate.threshold = self.threshold;
        (self.sink.events)(vec![SpeechEvent::Gap]);
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn status(&self, text: impl Into<String>) {
        (self.sink.status)(text.into());
    }

    fn finish(self) {
        (self.sink.finished)();
    }
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
        let mut listener = Listener::new(threshold, sink);
        let began = Instant::now();
        for (index, chunk) in samples.chunks(CHUNK).enumerate() {
            if stopping.load(Ordering::Relaxed) {
                return;
            }
            listener.hear(chunk);
            let due = began + Duration::from_millis((index as u64 + 1) * 100);
            thread::sleep(due.saturating_duration_since(Instant::now()));
        }
        listener.finish();
    });
    Ok(Capture {
        stop,
        worker: Some(worker),
    })
}

/// Runs a capture backend on its own thread. Its first step reports whether audio can be opened,
/// so Start can show the reason straight away instead of listening to nothing.
#[cfg(windows)]
fn spawn(
    threshold: f32,
    sink: Sink,
    run: impl FnOnce(&mut Listener, &AtomicBool, mpsc::Sender<Result<(), String>>) + Send + 'static,
) -> Result<Capture, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let (opened, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut listener = Listener::new(threshold, sink);
        run(&mut listener, &stopping, opened);
    });
    result
        .recv()
        .map_err(|_| "Audio capture stopped while starting.".to_string())??;
    Ok(Capture {
        stop,
        worker: Some(worker),
    })
}

/// Captures what this computer is playing.
#[cfg(windows)]
pub fn live(threshold: f32, sink: Sink) -> Result<Capture, String> {
    spawn(threshold, sink, wasapi::run)
}

#[cfg(not(windows))]
pub fn live(_threshold: f32, _sink: Sink) -> Result<Capture, String> {
    Err("This build has no live capture here. Run with --replay and a recording.".into())
}
