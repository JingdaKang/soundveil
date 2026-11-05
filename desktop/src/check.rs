//! `soundveil --capture-check 5` listens for that many seconds and reports what it heard, with no
//! model or window. It's the quickest way to find out whether capture works on a computer: play
//! something first. It fails if nothing came through.

use crate::capture::{self, Sink};
use soundveil_core::{SAMPLE_RATE, SpeechEvent};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const QUIET_THRESHOLD: f32 = 0.003;

pub fn run(seconds: f64) -> ExitCode {
    let voiced = Arc::new(AtomicUsize::new(0));
    let loudest = Arc::new(AtomicU32::new(0));
    let sink = {
        let (voiced, loudest) = (voiced.clone(), loudest.clone());
        Sink {
            events: Box::new(move |events| {
                for event in events {
                    if let SpeechEvent::Audio { samples, .. } = event {
                        voiced.fetch_add(samples.len(), Ordering::Relaxed);
                    }
                }
            }),
            level: Box::new(move |level| {
                loudest.fetch_max(level.to_bits(), Ordering::Relaxed);
            }),
            status: Box::new(|text| eprintln!("{text}")),
            finished: Box::new(|| {}),
        }
    };
    let capture = match capture::live(QUIET_THRESHOLD, sink) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    thread::sleep(Duration::from_secs_f64(seconds));
    capture.stop();
    let loudest = f32::from_bits(loudest.load(Ordering::Relaxed));
    let voiced = voiced.load(Ordering::Relaxed) as f64 / SAMPLE_RATE as f64;
    println!("Loudest level {loudest:.4}, {voiced:.1} s above the speech threshold.");
    if loudest > 0.0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("Nothing but silence came through.");
        ExitCode::FAILURE
    }
}
