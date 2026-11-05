//! Runs recognition on its own thread.
//!
//! Capture adds audio and this thread decodes one rolling pass at a time, always over the newest
//! audio, so a slow pass delays captions by no more than its own length. If inference stalls, the
//! transcriber's bounded buffer drops the oldest audio and that coverage is lost.

use soundveil_core::{
    CaptionUpdate, RollingTranscriber, SpeechEvent, UpdateSpeed, update_interval,
};
use soundveil_whisper::Recognizer;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct State {
    transcriber: RollingTranscriber,
    speed: UpdateSpeed,
    enabled: bool,
}

struct Shared {
    state: Mutex<State>,
    work: Condvar,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

/// What the engine reports back. Both are called on its thread.
pub struct Reports {
    pub update: Box<dyn Fn(CaptionUpdate) + Send>,
    pub status: Box<dyn Fn(String) + Send>,
}

impl Engine {
    /// Loads the model, which can take up to a minute, then starts waiting for audio.
    pub fn start(model: PathBuf, speed: UpdateSpeed, reports: Reports) -> Result<Self, String> {
        let mut transcriber = RollingTranscriber::default();
        transcriber.step = update_interval(speed, 0.0, false);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                transcriber,
                speed,
                enabled: true,
            }),
            work: Condvar::new(),
        });
        let (loaded, result) = mpsc::channel();
        let worker = {
            let shared = shared.clone();
            thread::spawn(move || match Recognizer::new(&model, true) {
                Some(recognizer) => {
                    let _ = loaded.send(Ok(()));
                    run(&shared, recognizer, &reports);
                }
                None => {
                    let message = "Cannot load model. Check the download and available memory.";
                    let _ = loaded.send(Err(message.to_string()));
                }
            })
        };
        result
            .recv()
            .map_err(|_| "The recognizer stopped while loading.".to_string())??;
        Ok(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn submit(&self, events: Vec<SpeechEvent>) {
        let mut state = self.shared.lock();
        if state.enabled {
            for event in events {
                state.transcriber.ingest(event);
            }
            self.shared.work.notify_one();
        }
    }

    /// Stops after the pass in progress and returns the words still on screen.
    pub fn stop(&self) -> CaptionUpdate {
        let update = {
            let mut state = self.shared.lock();
            state.enabled = false;
            state.transcriber.finish()
        };
        self.shared.work.notify_one();
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
        update
    }
}

fn run(shared: &Shared, mut recognizer: Recognizer, reports: &Reports) {
    let mut last_status = Instant::now() - Duration::from_secs(1);
    loop {
        let job = {
            let mut state = shared.lock();
            loop {
                if !state.enabled {
                    return;
                }
                if let Some(job) = state.transcriber.next_job() {
                    break job;
                }
                state = shared
                    .work
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let began = Instant::now();
        let words = recognizer.decode(&job);
        let took = began.elapsed().as_secs_f64();
        let failed = words.is_none();
        let (update, skipped, interval) = {
            let mut state = shared.lock();
            if !state.enabled {
                return;
            }
            let update = state.transcriber.complete(&job, words.unwrap_or_default());
            if !job.is_final {
                state.transcriber.step = update_interval(state.speed, took, false);
            }
            (update, state.transcriber.skipped(), state.transcriber.step)
        };
        if failed {
            (reports.status)(
                "Recognition failed. Stop and start again, or choose another model.".into(),
            );
            continue;
        }
        (reports.update)(update);
        if last_status.elapsed() >= Duration::from_secs(1) {
            last_status = Instant::now();
            let lost = if skipped >= 1.0 {
                format!(" · {skipped:.0} s skipped")
            } else {
                String::new()
            };
            let behind = took + interval / 2.0;
            (reports.status)(format!("Listening · about {behind:.1} s behind{lost}"));
        }
    }
}
