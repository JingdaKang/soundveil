//! Caption logic shared by every Soundveil app.
//!
//! This crate is pure: it takes audio samples and decoded words and returns what to show.
//! Capture, inference, and drawing belong to each platform, so everything here can be
//! tested without audio, a model, or a window.

pub mod caption_style;
pub mod expressive;
pub mod model;
pub mod pipeline;
pub mod release;
pub mod resize;
pub mod rolling;
pub mod shortcuts;
pub mod tokens;

pub use caption_style::{CaptionColor, CaptionPreset, CaptionStyle};
pub use expressive::{WordStyle, cue_label, word_styles};
pub use model::Model;
pub use pipeline::{Caption, SpeechEvent, SpeechGate, Word};
pub use release::{Release, Version};
pub use resize::{ResizeEdges, ResizeZones};
pub use rolling::{
    CaptionUpdate, DecodeJob, PlacedWord, RollingTranscriber, UpdateSpeed, WordPlacer,
    reveal_interval, update_interval,
};
pub use shortcuts::{Held, Keyboard, Shortcut, ShortcutAction, ShortcutMap};
pub use tokens::words_from_tokens;

/// Whisper's input rate. All audio in this crate is mono at this rate.
pub const SAMPLE_RATE: usize = 16_000;
