//! Caption logic shared by every Soundveil app.
//!
//! This crate is pure: it takes audio samples and decoded words and returns what to show.
//! Capture, inference, and drawing belong to each platform, so everything here can be
//! tested without audio, a model, or a window.

pub mod pipeline;

pub use pipeline::{Caption, SpeechEvent, SpeechGate, Word};

/// Whisper's input rate. All audio in this crate is mono at this rate.
pub const SAMPLE_RATE: usize = 16_000;
